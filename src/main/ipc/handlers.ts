import { match } from 'ts-pattern'
import {
  ipcMain,
  dialog,
  shell,
  BrowserWindow,
  systemPreferences,
  type IpcMainInvokeEvent,
} from 'electron'
import os from 'os'
import fs from 'fs'
import path from 'path'
import { ok, err, type Result } from 'neverthrow'
import type { PtyManager } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { LayoutStore } from '../db/LayoutStore'
import type { OnboardingStore } from '../db/OnboardingStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import type { AgentSessionManager } from '../agents/AgentSessionManager'
import type { WindowManager } from '../WindowManager'
import type { BrowserManager } from '../browser/BrowserManager'
import type { CredentialStore, Credential } from '../db/CredentialStore'
import { TmuxManager } from '../pty/TmuxManager'
import { execFile } from 'child_process'
import { promisify } from 'util'
import { GitRepository, type GitInfo } from '../git/GitRepository'
import { getLoginEnv } from '../shell/loginEnv'
import { GitWatcher, type GitRefreshFlags } from '../git/GitWatcher'
import { FileTreeWatcher } from '../fileWatcher/FileTreeWatcher'
import { DEFAULT_IGNORE_PATTERNS } from '../fileWatcher/defaults'
import { fileWatcherErrorMessage } from '../fileWatcher/errors'
import { runWorktreeSetup } from '../worktree/WorktreeSetupRunner'

const execFileAsync = promisify(execFile)
import type { WorktreeSetupAction } from '../db/types'
import { generateCommitMessage } from '../ai/commitMessageGenerator'
import type { TaskTrackerManager } from '../taskTracker/TaskTrackerManager'
import type { RepoConfigManager } from '../taskTracker/RepoConfigManager'
import type { GlobalConfigManager } from '../taskTracker/GlobalConfigManager'
import type { KeychainTokenStore } from '../taskTracker/KeychainTokenStore'
import type {
  TaskTrackerProvider,
  TrackerTask,
  RepoConfig,
  ResolvedConfig,
} from '../taskTracker/types'
import { taskTrackerErrorMessage } from '../taskTracker/errors'
import { mergeConfigs } from '../taskTracker/configMerge'
import { cascadeBounds } from '../windowBounds'
import { gitErrorMessage } from '../git/errors'

function unwrapOrThrow<T, E>(result: Result<T, E>, toMessage: (e: E) => string): T {
  if (result.isErr()) throw new Error(toMessage(result.error))
  return result.value
}
import {
  buildVariables,
  renderBranchName,
  renderPreview,
  getAvailablePlaceholders,
  validateTemplate,
  resolveBranchType,
  BRANCH_TYPE_OPTIONS,
} from '../taskTracker/branchTemplate'
import { createPullRequest, buildPRConfig } from '../taskTracker/prCreation'
import { getBranchTemplate, getPRTemplate } from '../taskTracker/configDefaults'
import type { GitHubService } from '../github/GitHubService'
import { gitHubErrorMessage } from '../github/errors'
import type { RemoteSessionService } from '../remote/RemoteSessionService'
import { remoteServerErrorMessage } from '../remote/errors'
import type { RunConfigManager } from '../runConfig/RunConfigManager'
import { runConfigErrorMessage } from '../runConfig/errors'
import type { SkillRegistry } from '../skills/SkillRegistry'
import type { SkillInstaller } from '../skills/SkillInstaller'
import type { SkillStore } from '../skills/SkillStore'
import type { SkillInstallOptions, SkillListOptions } from '../skills/types'
import { skillErrorMessage } from '../skills/errors'
import type { SkillError } from '../skills/errors'
import { getTransformer } from '../skills/SkillTransformer'
import { scanSkills } from '../skills/SkillScanner'
import type { SkillAgentTarget } from '../skills/types'
import type { ProfileStore } from '../profiles/ProfileStore'
import { profileToReader } from '../profiles/ProfileStore'
import { profileErrorMessage } from '../profiles/errors'
import { KNOWN_AGENT_TYPES, type ProfileInput } from '../profiles/types'
import type { SettingsExportService } from '../settings/SettingsExport'
import { settingsExportErrorMessage } from '../settings/errors'
import type { AgentType, PreferencesReader } from '../agents/types'
import { resolveShell } from '../pty/PtyManager'

function shellExecArgs(command: string): { command: string; args: string[] } {
  const shell = resolveShell()
  const flag = os.platform() === 'win32' ? '-Command' : '-lc'
  return { command: shell.command, args: [flag, command] }
}

// Session-level flag: once the user has successfully authenticated to reveal
// a saved credential in the current app session, subsequent autofills reuse
// that authentication instead of prompting the OS every time. Matches Chrome's
// autofill behavior and is cleared automatically on app quit (in-memory only).
// Only the 'autofill' code path reads or writes this — the Settings "Reveal
// Password" UI always re-authenticates.
let credentialSessionAuthenticated = false

// Session cache of already-decrypted credentials (keyed by id). Each cache hit
// avoids a fresh safeStorage.decryptString() call, which on macOS triggers a
// Keychain prompt for the "Safe Storage" item whenever the app binary
// signature changes (e.g. after a rebuild). Only the 'autofill' path uses
// this cache; 'reveal' always fetches fresh. Cleared on any save/delete/import
// so stale plaintext is never returned, and on app quit.
const credentialSessionCache = new Map<string, Credential>()

// Dedupe concurrent first-use OS auth prompts for the autofill path. When two
// autofill requests race past `credentialSessionAuthenticated === false` at
// the same time (e.g. a form with both username and password fields), they
// share one in-flight prompt instead of stacking two Touch ID dialogs.
let credentialAutofillAuthInflight: Promise<boolean> | null = null

async function runCredentialOsAuth(event: IpcMainInvokeEvent, domain: string): Promise<boolean> {
  return match(process.platform)
    .with('darwin', async () => {
      try {
        await systemPreferences.promptTouchID('reveal a saved password')
        return true
      } catch {
        return false
      }
    })
    .with('win32', async () => {
      try {
        const ps = `
          Add-Type -AssemblyName System.Runtime.WindowsRuntime
          $null = [Windows.Security.Credentials.UI.UserConsentVerifier,Windows.Security.Credentials.UI,ContentType=WindowsRuntime]
          $result = [Windows.Security.Credentials.UI.UserConsentVerifier]::RequestVerificationAsync('Canopy wants to reveal a saved password').GetAwaiter().GetResult()
          if ($result -ne 'Verified') { exit 1 }
        `
        await execFileAsync('powershell', ['-NoProfile', '-Command', ps])
        return true
      } catch {
        return false
      }
    })
    .otherwise(async () => {
      const win = BrowserWindow.fromWebContents(event.sender) ?? BrowserWindow.getFocusedWindow()
      if (!win) return false
      const { response } = await dialog.showMessageBox(win, {
        type: 'warning',
        buttons: ['Reveal Password', 'Cancel'],
        defaultId: 1,
        cancelId: 1,
        title: 'Authentication Required',
        message: 'Reveal saved password?',
        detail: `You are about to reveal the password for ${domain}. Make sure no one is looking at your screen.`,
      })
      return response === 0
    })
}

function resolveShellArgs(): string[] {
  if (os.platform() === 'win32') return []
  return ['--login']
}

export function registerIpcHandlers(
  ptyManager: PtyManager,
  wsBridge: WsBridge,
  workspaceStore: WorkspaceStore,
  preferencesStore: PreferencesStore,
  layoutStore: LayoutStore,
  toolRegistry: ToolRegistry,
  agentSessionManager: AgentSessionManager,
  windowManager: WindowManager,
  browserManager: BrowserManager,
  credentialStore: CredentialStore,
  onboardingStore: OnboardingStore,
  tmuxManager: TmuxManager,
  taskTrackerManager: TaskTrackerManager,
  repoConfigManager: RepoConfigManager,
  globalConfigManager: GlobalConfigManager,
  keychainTokenStore: KeychainTokenStore,
  gitHubService: GitHubService,
  remoteSessionService: RemoteSessionService,
  runConfigManager: RunConfigManager,
  skillRegistry: SkillRegistry,
  skillInstaller: SkillInstaller,
  skillStore: SkillStore,
  profileStore: ProfileStore,
  settingsExportService: SettingsExportService,
): void {
  function broadcastToolsChanged(): void {
    const tools = toolRegistry.getAll()
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.send('tools:changed', tools)
    }
  }

  function broadcastSkillsChanged(): void {
    const skills = JSON.parse(JSON.stringify(skillRegistry.getAll()))
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.send('skills:changed', skills)
    }
  }

  async function broadcastProfilesChanged(): Promise<void> {
    const list = (await profileStore.list()).unwrapOr([])
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.send('profile:changed', list)
    }
  }

  function persistWindowConfigs(): void {
    const configs = windowManager.getAllWindowConfigs()
    if (configs.length > 0) {
      preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
    } else {
      preferencesStore.delete('openWindowConfigs')
    }
  }

  // --- PTY ---

  ipcMain.handle(
    'pty:spawn',
    async (event, options?: { cols?: number; rows?: number; cwd?: string }) => {
      const sender = event.sender
      const session = ptyManager.spawn(options)
      const wsUrl = await wsBridge.create(session.id, session.pty)

      windowManager.trackPtySession(sender.id, session.id)

      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', { sessionId: session.id, exitCode, signal })
        }
        windowManager.untrackPtySession(sender.id, session.id)
      })

      return { sessionId: session.id, wsUrl }
    },
  )

  ipcMain.handle(
    'pty:resize',
    (_event, payload: { sessionId: string; cols: number; rows: number }) => {
      // Validate dimensions: positive integers within sane terminal bounds.
      // node-pty will otherwise throw on NaN / negative / huge values and
      // a malformed payload would be broadcast to every window as-is.
      const cols = payload?.cols
      const rows = payload?.rows
      if (
        !Number.isInteger(cols) ||
        !Number.isInteger(rows) ||
        cols < 1 ||
        rows < 1 ||
        cols > 10_000 ||
        rows > 10_000
      ) {
        return
      }
      ptyManager.resize(payload.sessionId, cols, rows)
      // Broadcast the new dimensions to every open window so the remote
      // host controller (running inside the host renderer) can relay them
      // to any connected WebRTC peer. Without this, a peer's xterm stays
      // at the PTY's original cols/rows and any cursor positioning escape
      // sequence the shell/CLI emits lands in the wrong column on the
      // peer's screen.
      for (const w of BrowserWindow.getAllWindows()) {
        if (!w.isDestroyed()) {
          w.webContents.send('pty:resized', payload)
        }
      }
    },
  )

  ipcMain.handle('pty:kill', async (_event, payload: { sessionId: string; killTmux?: boolean }) => {
    const tmuxName = ptyManager.getTmuxSessionName(payload.sessionId)
    // Kill tmux BEFORE PTY so the pty:exit handler sees the session as dead
    // (otherwise handlePtyExit checks tmuxHasSession while it's still alive)
    if (payload.killTmux && tmuxName && TmuxManager.isCanopySession(tmuxName)) {
      try {
        await tmuxManager.killSession(tmuxName)
      } catch {
        // Session may already be gone
      }
    }
    wsBridge.destroy(payload.sessionId)
    ptyManager.kill(payload.sessionId)
  })

  ipcMain.handle('pty:write', (_event, payload: { sessionId: string; data: string }) => {
    ptyManager.write(payload.sessionId, payload.data)
  })

  ipcMain.handle('pty:hasChildProcess', (_event, payload: { sessionId: string }) => {
    return ptyManager.hasChildProcess(payload.sessionId)
  })

  ipcMain.handle('pty:getDimensions', (_event, payload: { sessionId: string }) => {
    return ptyManager.getDimensions(payload.sessionId)
  })

  // --- Tmux ---

  function validateTmuxName(name: string): void {
    if (!/^[\w-]+$/.test(name)) {
      throw new Error('Invalid tmux session name: only letters, digits, underscores, and dashes')
    }
  }

  ipcMain.handle('tmux:isAvailable', async () => {
    return tmuxManager.isAvailable()
  })

  ipcMain.handle('tmux:getVersion', async () => {
    return tmuxManager.getVersion()
  })

  ipcMain.handle('tmux:listSessions', async () => {
    return tmuxManager.listSessions()
  })

  ipcMain.handle('tmux:hasSession', async (_event, payload: { name: string }) => {
    validateTmuxName(payload.name)
    return tmuxManager.hasSession(payload.name)
  })

  ipcMain.handle(
    'tmux:attach',
    async (event, payload: { tmuxSessionName: string; cols?: number; rows?: number }) => {
      validateTmuxName(payload.tmuxSessionName)
      const sender = event.sender
      const attach = tmuxManager.attachArgs(payload.tmuxSessionName)
      const session = ptyManager.spawn({
        command: attach.command,
        args: attach.args,
        cols: payload.cols,
        rows: payload.rows,
        tmuxSessionName: payload.tmuxSessionName,
      })
      const wsUrl = await wsBridge.create(session.id, session.pty)

      windowManager.trackPtySession(sender.id, session.id)

      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', {
            sessionId: session.id,
            exitCode,
            signal,
            tmuxSessionName: payload.tmuxSessionName,
          })
        }
        windowManager.untrackPtySession(sender.id, session.id)
      })

      return { sessionId: session.id, wsUrl }
    },
  )

  ipcMain.handle('tmux:detach', (_event, payload: { sessionId: string }) => {
    const tmuxName = ptyManager.getTmuxSessionName(payload.sessionId)
    wsBridge.destroy(payload.sessionId)
    ptyManager.kill(payload.sessionId)
    return { tmuxSessionName: tmuxName }
  })

  ipcMain.handle('tmux:killSession', async (_event, payload: { name: string }) => {
    validateTmuxName(payload.name)
    await tmuxManager.killSession(payload.name)
  })

  ipcMain.handle(
    'tmux:renameSession',
    async (_event, payload: { oldName: string; newName: string }) => {
      validateTmuxName(payload.oldName)
      validateTmuxName(payload.newName)
      await tmuxManager.renameSession(payload.oldName, payload.newName)
    },
  )

  // --- Tool Spawning ---

  ipcMain.handle(
    'tool:spawn',
    async (
      event,
      payload: {
        toolId: string
        worktreePath: string
        cols?: number
        rows?: number
        workspaceName?: string
        branch?: string
        resumeSessionId?: string
        profileId?: string
      },
    ) => {
      const sender = event.sender
      const tool = toolRegistry.get(payload.toolId)
      if (!tool) throw new Error(`Unknown tool: ${payload.toolId}`)

      let command = toolRegistry.resolveCommand(tool)
      const isShell = tool.id === 'shell' || tool.command === 'shell'
      const isAgent = agentSessionManager.isAgentTool(tool.id)
      let args = isShell ? resolveShellArgs() : [...tool.args]
      let env: Record<string, string> | undefined

      let agentTempId: string | undefined
      if (isAgent) {
        const senderWindow = BrowserWindow.fromWebContents(sender)
        if (!senderWindow) throw new Error('No window for agent session')

        // Resolve preferences reader: profile shim if profileId is set,
        // otherwise the global preferencesStore (legacy path).
        let prefsReader: PreferencesReader = preferencesStore
        if (payload.profileId) {
          const profileResult = await profileStore.getInternal(payload.profileId)
          if (profileResult.isErr()) {
            throw new Error(profileErrorMessage(profileResult.error))
          }
          const profile = profileResult.value
          if (profile.agentType !== tool.id) {
            throw new Error(`Profile ${profile.name} is for ${profile.agentType}, not ${tool.id}`)
          }
          prefsReader = profileToReader(profile, preferencesStore)
        }

        // Parse settings.json overrides from prefs (via shim when profile-bound)
        let settingsOverrides: Record<string, unknown> | undefined
        const settingsJsonRaw = prefsReader.get(`${tool.id}.settingsJson`)
        if (settingsJsonRaw) {
          try {
            settingsOverrides = JSON.parse(settingsJsonRaw) as Record<string, unknown>
          } catch {
            // Invalid JSON
          }
        }

        const agentSession = await agentSessionManager.createSession(
          tool.id,
          payload.worktreePath,
          payload.workspaceName ?? '',
          payload.branch ?? null,
          senderWindow,
          settingsOverrides,
        )
        args = [...agentSession.settingsArgs, ...args]
        if (payload.resumeSessionId) {
          args.push(...agentSessionManager.getResumeArgs(tool.id, payload.resumeSessionId))
        }
        args.push(...agentSessionManager.getCliArgs(tool.id, prefsReader))
        env = {
          CANOPY_HOOK_PORT: String(agentSession.hookPort),
          CANOPY_HOOK_PATH: agentSession.hookPath,
          CANOPY_HOOK_TOKEN: agentSession.hookAuthToken,
          ...agentSession.settingsEnv,
          ...agentSessionManager.getEnvVars(tool.id, prefsReader),
        }
        agentTempId = agentSession.tempId
      }

      // Tmux integration for all tool sessions
      let tmuxSessionName: string | undefined
      const tmuxEnabled = preferencesStore.get('tmux.enabled') === 'true'
      if (tmuxEnabled && (await tmuxManager.isAvailable())) {
        const ws = workspaceStore.getByPath(payload.worktreePath)
        const wsId = ws?.id ?? 'default'
        tmuxSessionName = TmuxManager.sessionName(wsId)
        const tmuxMouse = preferencesStore.get('tmux.mouse') === 'true'
        await tmuxManager.newSession({
          name: tmuxSessionName,
          cwd: payload.worktreePath,
          shell: command,
          shellArgs: args,
          cols: payload.cols,
          rows: payload.rows,
          mouse: tmuxMouse,
          env,
        })
        const attach = tmuxManager.attachArgs(tmuxSessionName)
        command = attach.command
        args = attach.args
      }

      const session = ptyManager.spawn({
        command,
        args,
        cwd: payload.worktreePath,
        cols: payload.cols,
        rows: payload.rows,
        env,
        tmuxSessionName,
      })

      if (isAgent && agentTempId) {
        agentSessionManager.rekey(agentTempId, session.id)
      }

      const wsUrl = await wsBridge.create(session.id, session.pty)

      windowManager.trackPtySession(sender.id, session.id)

      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', {
            sessionId: session.id,
            exitCode,
            signal,
            tmuxSessionName: session.tmuxSessionName,
          })
        }
        windowManager.untrackPtySession(sender.id, session.id)
        if (isAgent) {
          agentSessionManager.destroySession(session.id)
        }
      })

      return {
        sessionId: session.id,
        wsUrl,
        toolId: tool.id,
        toolName: tool.name,
        tmuxSessionName,
      }
    },
  )

  ipcMain.handle('agent:updateTitle', (_event, payload: { sessionId: string; title: string }) => {
    agentSessionManager.updateProcessTitle(payload.sessionId, payload.title)
  })

  // --- Workspaces ---

  ipcMain.handle('db:workspace:list', (_event, payload?: { limit?: number }) => {
    return workspaceStore.list(payload?.limit)
  })

  ipcMain.handle('db:workspace:get', (_event, payload: { id: string }) => {
    return workspaceStore.get(payload.id) ?? null
  })

  ipcMain.handle('db:workspace:getByPath', (_event, payload: { path: string }) => {
    return workspaceStore.getByPath(payload.path) ?? null
  })

  ipcMain.handle(
    'db:workspace:upsert',
    (_event, payload: { path: string; name: string; isGitRepo: boolean }) => {
      return workspaceStore.upsert(payload)
    },
  )

  ipcMain.handle('db:workspace:remove', (_event, payload: { id: string }) => {
    workspaceStore.remove(payload.id)
  })

  ipcMain.handle('db:workspace:touch', (_event, payload: { id: string }) => {
    workspaceStore.touch(payload.id)
  })

  // --- Preferences ---

  ipcMain.handle('db:prefs:get', (_event, payload: { key: string }) => {
    return preferencesStore.get(payload.key)
  })

  ipcMain.handle('db:prefs:set', (_event, payload: { key: string; value: string }) => {
    preferencesStore.set(payload.key, payload.value)
    if (payload.key === 'remote.enabled' && payload.value === 'false') {
      void remoteSessionService.stop()
    }
  })

  ipcMain.handle('db:prefs:getAll', () => {
    return preferencesStore.getAll()
  })

  ipcMain.handle('db:prefs:delete', (_event, payload: { key: string }) => {
    preferencesStore.delete(payload.key)
  })

  // --- Settings Export / Import ---

  ipcMain.handle('settings:export', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (!win || win.isDestroyed()) return null

    const today = new Date().toISOString().slice(0, 10)
    const defaultFilename = `canopy-settings-${today}.json`

    const saveResult = await dialog.showSaveDialog(win, {
      title: 'Export Canopy Settings',
      defaultPath: defaultFilename,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    })
    if (saveResult.canceled || !saveResult.filePath) return null

    const buildResult = await settingsExportService.buildExport()
    const file = unwrapOrThrow(buildResult, settingsExportErrorMessage)
    const json = JSON.stringify(file, null, 2)

    try {
      await fs.promises.writeFile(saveResult.filePath, json, { encoding: 'utf8', mode: 0o600 })
    } catch (e) {
      throw new Error(
        settingsExportErrorMessage({
          _tag: 'ExportWriteError',
          reason: e instanceof Error ? e.message : String(e),
        }),
      )
    }

    return {
      path: saveResult.filePath,
      counts: {
        preferences: Object.keys(file.preferences).length,
        profiles: file.profiles.length,
        credentials: file.credentials.length,
        customTools: file.customTools.length,
      },
    }
  })

  ipcMain.handle('settings:import', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (!win || win.isDestroyed()) return null

    const openResult = await dialog.showOpenDialog(win, {
      title: 'Import Canopy Settings',
      filters: [{ name: 'JSON', extensions: ['json'] }],
      properties: ['openFile'],
    })
    if (openResult.canceled || openResult.filePaths.length === 0) return null

    let raw: string
    try {
      raw = await fs.promises.readFile(openResult.filePaths[0], 'utf8')
    } catch (e) {
      throw new Error(
        settingsExportErrorMessage({
          _tag: 'ImportReadError',
          reason: e instanceof Error ? e.message : String(e),
        }),
      )
    }

    let parsed: unknown
    try {
      parsed = JSON.parse(raw)
    } catch (e) {
      throw new Error(
        settingsExportErrorMessage({
          _tag: 'ImportParseError',
          reason: e instanceof Error ? e.message : String(e),
        }),
      )
    }

    const applyResult = await settingsExportService.applyImport(parsed)
    const counts = unwrapOrThrow(applyResult, settingsExportErrorMessage)
    credentialSessionCache.clear()

    await broadcastProfilesChanged()
    broadcastToolsChanged()

    return { counts }
  })

  // --- Tools ---

  ipcMain.handle('tools:list', () => {
    return toolRegistry.getAll()
  })

  ipcMain.handle('tools:get', (_event, payload: { id: string }) => {
    return toolRegistry.get(payload.id) ?? null
  })

  ipcMain.handle('tools:checkAvailability', async () => {
    return toolRegistry.checkAvailability()
  })

  // --- Agent Profiles ---

  const KNOWN_AGENT_TYPES_SET: ReadonlySet<AgentType> = new Set(KNOWN_AGENT_TYPES)

  ipcMain.handle('profile:list', async (_event, payload?: { agentType?: AgentType }) => {
    return (await profileStore.list(payload?.agentType)).unwrapOr([])
  })

  ipcMain.handle('profile:get', async (_event, payload: { id: string }) => {
    if (!payload || typeof payload.id !== 'string') {
      throw new Error('profile:get requires a string id')
    }
    return (await profileStore.get(payload.id)).unwrapOr(null)
  })

  ipcMain.handle('profile:save', async (_event, input: ProfileInput) => {
    if (!input || typeof input !== 'object') {
      throw new Error('profile:save requires an input object')
    }
    if (typeof input.name !== 'string') {
      throw new Error('profile:save: name must be a string')
    }
    if (typeof input.agentType !== 'string' || !KNOWN_AGENT_TYPES_SET.has(input.agentType)) {
      throw new Error(`profile:save: unknown agentType "${String(input.agentType)}"`)
    }
    if (input.id !== undefined && typeof input.id !== 'string') {
      throw new Error('profile:save: id must be a string when provided')
    }
    if (input.prefs !== undefined && (typeof input.prefs !== 'object' || input.prefs === null)) {
      throw new Error('profile:save: prefs must be an object')
    }
    if (input.apiKey !== undefined && input.apiKey !== null && typeof input.apiKey !== 'string') {
      throw new Error('profile:save: apiKey must be a string, null, or omitted')
    }

    const result = await profileStore.save(input)
    const profile = unwrapOrThrow(result, profileErrorMessage)
    await broadcastProfilesChanged()
    return profileStore.toMasked(profile)
  })

  ipcMain.handle('profile:delete', async (_event, payload: { id: string }) => {
    if (!payload || typeof payload.id !== 'string') {
      throw new Error('profile:delete requires a string id')
    }
    const result = await profileStore.delete(payload.id)
    unwrapOrThrow(result, profileErrorMessage)
    await broadcastProfilesChanged()
  })

  // --- Environment / Dependencies ---

  ipcMain.handle('env:checkDependencies', async (_event, payload: { tools: string[] }) => {
    const KNOWN_TOOLS = new Set(['claude', 'codex', 'gemini'])
    const requested = (payload.tools ?? []).filter((t) => KNOWN_TOOLS.has(t))

    const cmd = os.platform() === 'win32' ? 'where' : 'which'
    const env = getLoginEnv() ?? (process.env as Record<string, string>)

    const check = (binary: string): Promise<{ found: boolean; path?: string }> =>
      new Promise((resolve) => {
        execFile(cmd, [binary], { env }, (err, stdout) => {
          resolve(err ? { found: false } : { found: true, path: stdout.trim().split('\n')[0] })
        })
      })

    const binaries = [...new Set([...requested, 'git'])]
    const statuses = await Promise.all(binaries.map((b) => check(b)))
    const results: Record<string, { found: boolean; path?: string }> = {}
    binaries.forEach((b, i) => {
      results[b] = statuses[i]
    })

    return { results, platform: process.platform }
  })

  // --- App / Shell ---

  ipcMain.handle('app:homedir', () => os.homedir())

  ipcMain.handle('app:showInFolder', (_event, payload: { path: string }) => {
    shell.showItemInFolder(payload.path)
  })

  // --- App: Multi-window ---

  ipcMain.handle('app:newWindow', () => {
    windowManager.createWindow({
      bounds: cascadeBounds(windowManager.getLastFocusedBounds()),
    })
  })

  ipcMain.handle('app:setWorkspacePath', (event, payload: { path: string }) => {
    windowManager.addWorkspacePath(event.sender.id, payload.path)
    persistWindowConfigs()
  })

  ipcMain.handle('app:setActiveWorktree', (event, payload: { path: string }) => {
    windowManager.setActiveWorktree(event.sender.id, payload.path)
    persistWindowConfigs()
  })

  ipcMain.handle(
    'app:setFocusedAgentSession',
    (event, payload: { ptySessionId: string | null }) => {
      windowManager.setFocusedAgentSession(event.sender.id, payload.ptySessionId)
    },
  )

  ipcMain.handle('app:detachProject', (event, payload: { path: string }) => {
    const senderId = event.sender.id
    windowManager.removeWorkspacePath(senderId, payload.path)
    windowManager.disposeGitWatcher(senderId, payload.path)
    // Delete layouts so this workspace won't restore on next launch
    const ws = workspaceStore.getByPath(payload.path)
    if (ws) layoutStore.deleteAll(ws.id)
    persistWindowConfigs()
  })

  ipcMain.handle('app:focusWindowForPath', (event, payload: { path: string }) => {
    const existing = windowManager.getWindowForPath(payload.path)
    if (existing && existing.webContents.id !== event.sender.id) {
      if (existing.isMinimized()) existing.restore()
      existing.focus()
      return true
    }
    return false
  })

  ipcMain.handle('app:focusRendererWebContents', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (win && !win.isDestroyed()) {
      win.webContents.focus()
    }
  })

  // --- Dialog ---

  ipcMain.handle('dialog:openFolder', async (event, payload?: { defaultPath?: string }) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (!win || win.isDestroyed()) return null
    const result = await dialog.showOpenDialog(win, {
      properties: ['openDirectory', 'createDirectory'],
      ...(payload?.defaultPath ? { defaultPath: payload.defaultPath } : {}),
    })
    return result.canceled ? null : result.filePaths[0]
  })

  // --- Git ---

  const defaultGitInfo: GitInfo = {
    isGitRepo: false,
    repoRoot: null,
    branch: null,
    worktrees: [],
    isDirty: false,
    aheadBehind: null,
  }

  ipcMain.handle('git:detect', async (_event, payload: { path: string }) => {
    return GitRepository.detect(payload.path).unwrapOr(defaultGitInfo)
  })

  ipcMain.handle('git:worktrees', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.listWorktrees(payload.repoRoot).unwrapOr([])
  })

  ipcMain.handle('git:status', async (_event, payload: { path: string }) => {
    const branch = await GitRepository.getBranch(payload.path).unwrapOr(null)
    const isDirty = await GitRepository.isDirty(payload.path).unwrapOr(false)
    const aheadBehind = await GitRepository.getAheadBehind(payload.path).unwrapOr(null)
    return { branch, isDirty, aheadBehind }
  })

  ipcMain.handle('git:watch', async (event, payload: { repoRoot: string; snapshot?: GitInfo }) => {
    const senderId = event.sender.id

    // Dispose previous watcher for this specific repo only
    windowManager.disposeGitWatcher(senderId, payload.repoRoot)

    // Find workspace ID for cache updates
    const ws = workspaceStore.getByPath(payload.repoRoot)
    const workspaceId = ws?.id ?? null

    const watcher = new GitWatcher(
      payload.repoRoot,
      (info, changes: GitRefreshFlags) => {
        if (workspaceId) {
          workspaceStore.updateGitCache(workspaceId, {
            branch: info.branch,
            dirty: info.isDirty,
            aheadBehind: info.aheadBehind
              ? `${info.aheadBehind.ahead}/${info.aheadBehind.behind}`
              : null,
            worktreeCount: info.worktrees.length,
          })
        }
        if (!event.sender.isDestroyed()) {
          event.sender.send('git:changed', { ...info, repoRoot: payload.repoRoot, changes })
        }
      },
      payload.snapshot,
    )
    const startResult = await watcher.start()
    if (startResult.isErr()) {
      // Log but don't throw — git watching is best-effort, the renderer
      // can still query git state on demand if the watcher fails to start.
      console.warn(gitErrorMessage(startResult.error))
    }
    windowManager.setGitWatcher(senderId, payload.repoRoot, watcher)
  })

  ipcMain.handle('git:unwatch', (event, payload?: { repoRoot?: string }) => {
    if (payload?.repoRoot) {
      windowManager.disposeGitWatcher(event.sender.id, payload.repoRoot)
    } else {
      windowManager.disposeAllGitWatchers(event.sender.id)
    }
  })

  // --- File Tree Watcher ---

  function getIgnorePatterns(): string[] {
    const raw = preferencesStore.get('files.ignorePatterns')
    if (!raw) return [...DEFAULT_IGNORE_PATTERNS]
    try {
      const parsed = JSON.parse(raw)
      if (Array.isArray(parsed) && parsed.every((p) => typeof p === 'string')) {
        return parsed
      }
    } catch {
      // Invalid JSON in prefs — fall back to defaults
    }
    return [...DEFAULT_IGNORE_PATTERNS]
  }

  function validatePatternsPayload(patterns: unknown): string[] {
    if (!Array.isArray(patterns)) {
      throw new Error('Invalid patterns: must be an array of strings')
    }
    const result: string[] = []
    for (const p of patterns) {
      if (typeof p !== 'string') {
        throw new Error('Invalid patterns: all entries must be strings')
      }
      const trimmed = p.trim()
      if (trimmed) result.push(trimmed)
    }
    return result
  }

  ipcMain.handle('files:watch', async (event, payload: { repoRoot: string }) => {
    if (typeof payload?.repoRoot !== 'string' || !path.isAbsolute(payload.repoRoot)) {
      throw new Error('Invalid repoRoot: must be an absolute path string')
    }
    // Enforce that the watched path belongs to one of the window's workspaces
    await validatePathAccess(event.sender.id, payload.repoRoot)

    const senderId = event.sender.id

    // Only one watcher per window — dispose any previous one first
    windowManager.disposeFileWatcher(senderId)

    const watcher = new FileTreeWatcher(payload.repoRoot, (events) => {
      if (!event.sender.isDestroyed()) {
        event.sender.send('files:changed', { repoRoot: payload.repoRoot, events })
      }
    })

    const result = await watcher.start()
    if (result.isErr()) {
      throw new Error(fileWatcherErrorMessage(result.error))
    }
    windowManager.setFileWatcher(senderId, watcher)
  })

  ipcMain.handle('files:unwatch', (event) => {
    windowManager.disposeFileWatcher(event.sender.id)
  })

  ipcMain.handle('files:updateIgnorePatterns', (_event, payload: { patterns: unknown }) => {
    const patterns = validatePatternsPayload(payload?.patterns)
    preferencesStore.set('files.ignorePatterns', JSON.stringify(patterns))
    // No watcher restart needed — user patterns are now applied per-consumer
    // in the renderer (sidebar filters them, diff/changes panels see all
    // events). Watcher only honours hardcoded SAFETY_IGNORE_PATTERNS.
  })

  ipcMain.handle('files:getDefaultIgnorePatterns', () => {
    return [...DEFAULT_IGNORE_PATTERNS]
  })

  ipcMain.handle('git:init', async (_event, payload: { path: string }) => {
    await execFileAsync('git', ['init'], { cwd: payload.path })
    return GitRepository.detect(payload.path).unwrapOr(defaultGitInfo)
  })

  // --- Workspace Git Status Refresh ---

  ipcMain.handle(
    'db:workspace:refreshGitStatus',
    async (_event, payload: { id: string; path: string }) => {
      const info = await GitRepository.detect(payload.path).unwrapOr(defaultGitInfo)
      const aheadBehind = info.aheadBehind ? JSON.stringify(info.aheadBehind) : null
      workspaceStore.updateGitCache(payload.id, {
        branch: info.branch,
        dirty: info.isDirty,
        aheadBehind,
        worktreeCount: info.worktrees.length,
      })
      return workspaceStore.get(payload.id) ?? null
    },
  )

  // --- Git Operations ---

  ipcMain.handle(
    'git:commit',
    async (_event, payload: { repoRoot: string; message: string; stageAll?: boolean }) => {
      const result = await GitRepository.commit(payload.repoRoot, payload.message, payload.stageAll)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:push', async (_event, payload: { repoRoot: string }) => {
    const result = await GitRepository.push(payload.repoRoot)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:pull', async (_event, payload: { repoRoot: string; rebase: boolean }) => {
    const result = await GitRepository.pull(payload.repoRoot, payload.rebase)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:fetch', async (_event, payload: { repoRoot: string }) => {
    const result = await GitRepository.fetch(payload.repoRoot)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:fetchAll', async (_event, payload: { repoRoot: string }) => {
    const result = await GitRepository.fetchAll(payload.repoRoot)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:stash', async (_event, payload: { repoRoot: string }) => {
    const result = await GitRepository.stash(payload.repoRoot)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:stashPop', async (_event, payload: { repoRoot: string }) => {
    const result = await GitRepository.stashPop(payload.repoRoot)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:branches', async (_event, payload: { repoRoot: string }) => {
    const result = await GitRepository.listBranches(payload.repoRoot)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle(
    'git:branchCreate',
    async (_event, payload: { repoRoot: string; name: string; baseBranch: string }) => {
      const result = await GitRepository.createBranch(
        payload.repoRoot,
        payload.name,
        payload.baseBranch,
      )
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:checkout', async (_event, payload: { repoRoot: string; branch: string }) => {
    const result = await GitRepository.checkout(payload.repoRoot, payload.branch)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle(
    'git:branchDelete',
    async (_event, payload: { repoRoot: string; name: string; force: boolean }) => {
      const result = await GitRepository.deleteBranch(payload.repoRoot, payload.name, payload.force)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'git:branchDeleteRemote',
    async (_event, payload: { repoRoot: string; remote: string; name: string }) => {
      const result = await GitRepository.deleteRemoteBranch(
        payload.repoRoot,
        payload.remote,
        payload.name,
      )
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:pushInfo', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.getPushInfo(payload.repoRoot).unwrapOr(null)
  })

  ipcMain.handle(
    'git:branchMerged',
    async (_event, payload: { repoRoot: string; branch: string }) => {
      return GitRepository.isBranchMerged(payload.repoRoot, payload.branch).unwrapOr(false)
    },
  )

  ipcMain.handle(
    'git:worktreeAdd',
    async (
      _event,
      payload: { repoRoot: string; path: string; branch: string; baseBranch: string },
    ) => {
      const resolvedPath = payload.path.startsWith('~/')
        ? os.homedir() + payload.path.slice(1)
        : payload.path
      const result = await GitRepository.worktreeAdd(
        payload.repoRoot,
        resolvedPath,
        payload.branch,
        payload.baseBranch,
      )
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'git:worktreeCheckout',
    async (
      _event,
      payload: {
        repoRoot: string
        path: string
        branch: string
        createLocalTracking: boolean
      },
    ) => {
      const resolvedPath = payload.path.startsWith('~/')
        ? os.homedir() + payload.path.slice(1)
        : payload.path
      const result = await GitRepository.worktreeAddCheckout(
        payload.repoRoot,
        resolvedPath,
        payload.branch,
        payload.createLocalTracking,
      )
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'git:worktreeRemove',
    async (_event, payload: { repoRoot: string; path: string; force: boolean }) => {
      const result = await GitRepository.worktreeRemove(
        payload.repoRoot,
        payload.path,
        payload.force,
      )
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'git:unmergedCommits',
    async (_event, payload: { repoRoot: string; branch: string }) => {
      return GitRepository.getUnmergedCommits(payload.repoRoot, payload.branch).unwrapOr([])
    },
  )

  ipcMain.handle(
    'git:statusPorcelain',
    async (_event, payload: { repoRoot: string; worktreePath?: string }) => {
      return GitRepository.getStatusPorcelain(payload.repoRoot, payload.worktreePath).unwrapOr('')
    },
  )

  ipcMain.handle('git:diff', async (_event, payload: { repoRoot: string }) => {
    const result = await GitRepository.getDiffParsed(payload.repoRoot)
    return result.unwrapOr({ files: [] })
  })

  function validateFilePath(filePath: string): void {
    if (filePath.startsWith('-')) throw new Error('Invalid file path: must not start with -')
    if (filePath.startsWith('/')) throw new Error('Invalid file path: must be relative')
    if (filePath.includes('..')) throw new Error('Invalid file path: must not contain ..')
  }

  ipcMain.handle(
    'git:diffFile',
    async (_event, payload: { repoRoot: string; filePath: string }) => {
      validateFilePath(payload.filePath)
      const result = await GitRepository.getFileDiff(payload.repoRoot, payload.filePath)
      return result.unwrapOr({ files: [] })
    },
  )

  ipcMain.handle(
    'git:stageFile',
    async (_event, payload: { repoRoot: string; filePath: string }) => {
      validateFilePath(payload.filePath)
      const result = await GitRepository.stageFile(payload.repoRoot, payload.filePath)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'git:revertFile',
    async (_event, payload: { repoRoot: string; filePath: string }) => {
      validateFilePath(payload.filePath)
      const result = await GitRepository.revertFile(payload.repoRoot, payload.filePath)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:generateCommitMessage', async (_event, payload: { repoRoot: string }) => {
    const diff = await GitRepository.getDiff(payload.repoRoot).unwrapOr('')
    if (!diff.trim()) return null
    return generateCommitMessage(diff, preferencesStore)
  })

  ipcMain.handle(
    'git:createPR',
    async (
      _event,
      payload: {
        repoRoot: string
        title: string
        body: string
        baseRefName: string
        draft: boolean
      },
    ) => {
      const pushResult = await GitRepository.push(payload.repoRoot)
      if (pushResult.isErr()) {
        throw new Error(`Failed to push branch: ${gitErrorMessage(pushResult.error)}`)
      }

      const branch = await GitRepository.getBranch(payload.repoRoot).unwrapOr(null)
      if (!branch) throw new Error('Could not determine current branch')

      const args = [
        'pr',
        'create',
        '--title',
        payload.title,
        '--body',
        payload.body || '',
        '--base',
        payload.baseRefName,
        '--head',
        branch,
      ]
      if (payload.draft) args.push('--draft')

      try {
        const { stdout } = await execFileAsync('gh', args, { cwd: payload.repoRoot })
        return { url: stdout.trim() }
      } catch (err) {
        if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
          throw new Error(
            'GitHub CLI (gh) is not installed. Install it from cli.github.com or configure a GitHub connection in Preferences.',
          )
        }
        throw err
      }
    },
  )

  ipcMain.handle('git:getDefaultBranch', async (_event, payload: { repoRoot: string }) => {
    try {
      const { stdout } = await execFileAsync(
        'gh',
        ['repo', 'view', '--json', 'defaultBranchRef', '--jq', '.defaultBranchRef.name'],
        { cwd: payload.repoRoot },
      )
      return stdout.trim() || 'main'
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
        console.warn('[git:getDefaultBranch] gh CLI not found, falling back to "main"')
      }
      return 'main'
    }
  })

  // --- Layouts ---

  ipcMain.handle(
    'layout:save',
    (_event, payload: { workspaceId: string; worktreePath: string; layoutJson: string }) => {
      try {
        layoutStore.save(payload.workspaceId, payload.worktreePath, payload.layoutJson)
      } catch (error) {
        if (layoutStore.isClosed()) {
          // DB may already be closed during shutdown
          return
        }
        console.error('Failed to save layout:', error)
      }
    },
  )

  ipcMain.handle('layout:get', (_event, payload: { workspaceId: string; worktreePath: string }) => {
    try {
      return layoutStore.get(payload.workspaceId, payload.worktreePath)
    } catch (error) {
      if (layoutStore.isClosed()) {
        // DB may already be closed during shutdown
        return null
      }
      console.error('Failed to load layout:', error)
      throw error
    }
  })

  ipcMain.handle('layout:getAll', (_event, payload: { workspaceId: string }) => {
    try {
      return layoutStore.getAll(payload.workspaceId)
    } catch (error) {
      if (layoutStore.isClosed()) {
        // DB may already be closed during shutdown
        return []
      }
      console.error('Failed to load layouts:', error)
      throw error
    }
  })

  ipcMain.handle(
    'layout:delete',
    (_event, payload: { workspaceId: string; worktreePath: string }) => {
      try {
        layoutStore.delete(payload.workspaceId, payload.worktreePath)
      } catch (error) {
        if (layoutStore.isClosed()) {
          // DB may already be closed during shutdown
          return
        }
        console.error('Failed to delete layout:', error)
        throw error
      }
    },
  )

  // --- Custom Tools ---

  ipcMain.handle(
    'tools:addCustom',
    (
      _event,
      payload: {
        id: string
        name: string
        command: string
        args?: string[]
        icon?: string
        category?: string
      },
    ) => {
      toolRegistry.addCustom(payload)
      broadcastToolsChanged()
      return toolRegistry.getAll()
    },
  )

  ipcMain.handle('tools:removeCustom', (_event, payload: { id: string }) => {
    toolRegistry.removeCustom(payload.id)
    broadcastToolsChanged()
    return toolRegistry.getAll()
  })

  ipcMain.handle(
    'tools:updateCustom',
    (
      _event,
      payload: {
        id: string
        changes: {
          name?: string
          command?: string
          args?: string[]
          icon?: string
          category?: string
        }
      },
    ) => {
      toolRegistry.updateCustom(payload.id, payload.changes)
      broadcastToolsChanged()
      return toolRegistry.getAll()
    },
  )

  // --- Browser (<webview> management) ---

  ipcMain.handle(
    'browser:setup',
    (event, payload: { browserId: string; webContentsId: number }) => {
      const win = BrowserWindow.fromWebContents(event.sender)
      if (!win) throw new Error('No window for browser webview')
      browserManager.setup(payload.browserId, payload.webContentsId, win, event.sender)
    },
  )

  ipcMain.handle('browser:teardown', (_event, payload: { browserId: string }) => {
    browserManager.teardown(payload.browserId)
  })

  ipcMain.handle('browser:openDevTools', (_event, payload: { browserId: string }) => {
    browserManager.openDevTools(payload.browserId)
  })

  ipcMain.handle('browser:closeDevTools', (_event, payload: { browserId: string }) => {
    browserManager.closeDevTools(payload.browserId)
  })

  ipcMain.handle(
    'browser:setDevToolsBounds',
    (
      _event,
      payload: {
        browserId: string
        bounds: { x: number; y: number; width: number; height: number }
      },
    ) => {
      browserManager.setDevToolsBounds(payload.browserId, payload.bounds)
    },
  )

  ipcMain.handle(
    'browser:setDeviceEmulation',
    (
      _event,
      payload: {
        browserId: string
        device: { width: number; height: number; scaleFactor: number; mobile: boolean } | null
      },
    ) => {
      browserManager.setDeviceEmulation(payload.browserId, payload.device)
    },
  )

  ipcMain.handle(
    'browser:setBackgroundThrottling',
    (
      _event,
      payload: {
        browserId: string
        allowed: boolean
      },
    ) => {
      browserManager.setBackgroundThrottling(payload.browserId, payload.allowed)
    },
  )

  ipcMain.handle('browser:saveCaptureFile', (_event, payload: { buffer: Buffer }) => {
    return browserManager.saveCaptureFile(Buffer.from(payload.buffer))
  })

  // --- Credentials ---

  ipcMain.handle('credentials:getForDomain', (_event, payload: { domain: string }) => {
    return credentialStore.getForDomainMasked(payload.domain)
  })

  ipcMain.handle(
    'credentials:save',
    (_event, payload: { domain: string; username: string; password: string; title?: string }) => {
      credentialStore.save(payload.domain, payload.username, payload.password, payload.title)
      credentialSessionCache.clear()
    },
  )

  ipcMain.handle('credentials:delete', (_event, payload: { id: string }) => {
    credentialStore.delete(payload.id)
    credentialSessionCache.delete(payload.id)
  })

  ipcMain.handle('credentials:getAll', () => {
    return credentialStore.getAll()
  })

  ipcMain.handle(
    'browser:fillCredential',
    (_event, payload: { browserId: string; username: string; password: string }) => {
      browserManager.fillCredential(payload.browserId, payload.username, payload.password)
    },
  )

  ipcMain.handle(
    'credentials:getDecrypted',
    async (event, payload: { id: string; domain: string; purpose: 'autofill' | 'reveal' }) => {
      // Settings "Reveal Password" UI: always re-authenticate, never consult
      // the session cache or flag, never update them. Matches Chrome's
      // chrome://password-manager which re-prompts on every reveal regardless
      // of whether autofill has already happened this session.
      if (payload.purpose === 'reveal') {
        const authed = await runCredentialOsAuth(event, payload.domain)
        if (!authed) return null
        const cred = credentialStore.getById(payload.id)
        return cred && cred.domain === payload.domain ? cred : null
      }

      // Autofill path. Fast path: if we've already decrypted this credential
      // in the current session, return the cached plaintext without hitting
      // safeStorage or the OS auth prompt at all.
      const cached = credentialSessionCache.get(payload.id)
      if (cached && cached.domain === payload.domain) return cached

      // Require OS auth on first autofill of the session; subsequent autofills
      // reuse the session flag. Concurrent first-use calls dedupe through
      // credentialAutofillAuthInflight so only one Touch ID prompt appears
      // even when two requests race.
      const authed = credentialSessionAuthenticated
        ? true
        : await (credentialAutofillAuthInflight ??= runCredentialOsAuth(
            event,
            payload.domain,
          ).finally(() => {
            credentialAutofillAuthInflight = null
          }))
      if (!authed) return null
      credentialSessionAuthenticated = true
      // Fetch + decrypt only the requested credential (one safeStorage call),
      // instead of decrypting every credential stored for this domain.
      const cred = credentialStore.getById(payload.id)
      if (!cred || cred.domain !== payload.domain) return null
      credentialSessionCache.set(cred.id, cred)
      return cred
    },
  )

  // --- Filesystem ---

  /**
   * Returns true if a direct-child entry `name` should be hidden based on the
   * user's ignore patterns. Handles plain names (`node_modules`) and the first
   * segment of glob patterns (`dist/**` → hides a child named `dist`). More
   * complex globs like `**\/*.log` are left to the file watcher and ignored
   * here, since `fs:readDir` only sees immediate children.
   */
  function isIgnoredEntry(name: string, patterns: string[]): boolean {
    for (const pattern of patterns) {
      if (!pattern.includes('*') && !pattern.includes('?') && !pattern.includes('/')) {
        if (name === pattern) return true
        continue
      }
      const firstSegment = pattern.split('/')[0]
      if (
        firstSegment &&
        !firstSegment.includes('*') &&
        !firstSegment.includes('?') &&
        firstSegment === name
      ) {
        return true
      }
    }
    return false
  }

  async function validatePathAccess(wcId: number, targetPath: string): Promise<void> {
    const resolved = path.normalize(await fs.promises.realpath(targetPath))
    const allowed = windowManager.getWorkspacePaths(wcId)
    const ok = allowed.some((wp) => {
      const normalWp = path.normalize(wp)
      // Windows paths are case-insensitive
      if (process.platform === 'win32') {
        const r = resolved.toLowerCase()
        const w = normalWp.toLowerCase()
        return r === w || r.startsWith(w + path.sep)
      }
      return resolved === normalWp || resolved.startsWith(normalWp + path.sep)
    })
    if (!ok) throw new Error('Access denied: path outside workspace')
  }

  ipcMain.handle('fs:readDir', async (event, payload: { dirPath: string }) => {
    await validatePathAccess(event.sender.id, payload.dirPath)
    const entries = await fs.promises.readdir(payload.dirPath, { withFileTypes: true })
    const ignorePatterns = getIgnorePatterns()
    const filtered = entries.filter((e) => !isIgnoredEntry(e.name, ignorePatterns))
    const results = await Promise.all(
      filtered.map(async (entry) => {
        const isDir = entry.isDirectory()
        let size = 0
        if (!isDir) {
          try {
            const s = await fs.promises.stat(path.join(payload.dirPath, entry.name))
            size = s.size
          } catch {
            return null
          }
        }
        return { name: entry.name, isDirectory: isDir, size }
      }),
    )
    return results
      .filter((r): r is { name: string; isDirectory: boolean; size: number } => r !== null)
      .sort((a, b) => {
        if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
        return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
      })
  })

  ipcMain.handle('fs:readFile', async (event, payload: { filePath: string; maxBytes?: number }) => {
    await validatePathAccess(event.sender.id, payload.filePath)
    const maxBytes = Math.min(payload.maxBytes ?? 1_048_576, 10_485_760)
    // Sync stat too: closes the TOCTOU gap with the openSync below and removes
    // the last await between validatePathAccess and the fd trio.
    const size = fs.statSync(payload.filePath).size
    const readSize = Math.min(size, maxBytes)

    // Sync fd trio instead of async FileHandle: avoids holding a JS FileHandle
    // across multiple `await` points, which is the only call site in this
    // codebase that exposes us to FileHandle::CloseReq::Resolve races (#150).
    // A bounded read (≤10 MB) from local disk is fast enough to run inline.
    const fd = fs.openSync(payload.filePath, 'r')
    try {
      const buf = Buffer.alloc(readSize)
      let offset = 0
      while (offset < readSize) {
        const bytesRead = fs.readSync(fd, buf, offset, readSize - offset, offset)
        if (bytesRead === 0) break
        offset += bytesRead
      }

      // Binary detection: check first 8KB for null bytes
      const detectEnd = Math.min(offset, 8192)
      for (let i = 0; i < detectEnd; i++) {
        if (buf[i] === 0) return { binary: true, size }
      }

      return {
        content: buf.subarray(0, offset).toString('utf-8'),
        truncated: size > maxBytes,
        size,
        binary: false,
      }
    } finally {
      fs.closeSync(fd)
    }
  })

  // --- Shared config validation (used by both repo and global config handlers) ---

  const VALID_PROVIDERS = new Set(['jira', 'youtrack', 'github'])

  function isValidRepoConfig(c: unknown): c is RepoConfig {
    if (!c || typeof c !== 'object') return false
    const o = c as Record<string, unknown>
    return (
      o.version === 1 &&
      Array.isArray(o.trackers) &&
      (o.trackers as unknown[]).every(
        (t) =>
          t &&
          typeof (t as Record<string, unknown>).id === 'string' &&
          VALID_PROVIDERS.has(String((t as Record<string, unknown>).provider)) &&
          typeof (t as Record<string, unknown>).baseUrl === 'string' &&
          (!(t as Record<string, unknown>).baseUrl ||
            /^https?:\/\//.test(String((t as Record<string, unknown>).baseUrl))),
      ) &&
      !!o.filters &&
      typeof (o.filters as Record<string, unknown>).assignedToMe === 'boolean' &&
      Array.isArray((o.filters as Record<string, unknown>).statuses) &&
      ((o.filters as Record<string, unknown>).statuses as unknown[]).every(
        (s) => typeof s === 'string',
      ) &&
      typeof o.boardOverrides === 'object' &&
      (!o.branchTemplate ||
        typeof (o.branchTemplate as Record<string, unknown>).template === 'string') &&
      (!o.prTemplate || typeof (o.prTemplate as Record<string, unknown>).titleTemplate === 'string')
    )
  }

  // --- Repo Config ---

  ipcMain.handle('repoConfig:load', async (_event, payload: { repoRoot: string }) => {
    const result = await repoConfigManager.load(payload.repoRoot)
    return result.unwrapOr(null)
  })

  ipcMain.handle(
    'repoConfig:save',
    async (_event, payload: { repoRoot: string; config: unknown }) => {
      if (!isValidRepoConfig(payload.config)) {
        throw new Error('Invalid config: check version, trackers, filters, and template fields')
      }
      const result = await repoConfigManager.save(payload.repoRoot, payload.config)
      unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle('repoConfig:exists', async (_event, payload: { repoRoot: string }) => {
    return repoConfigManager.exists(payload.repoRoot)
  })

  ipcMain.handle('repoConfig:init', async (_event, payload: { repoRoot: string }) => {
    const result = await repoConfigManager.init(payload.repoRoot)
    return unwrapOrThrow(result, taskTrackerErrorMessage)
  })

  // --- Global Config ---

  ipcMain.handle('globalConfig:load', () => {
    return globalConfigManager.load()
  })

  ipcMain.handle('globalConfig:save', (_event, payload: { config: unknown }) => {
    if (!isValidRepoConfig(payload.config)) {
      throw new Error('Invalid config: check version, trackers, filters, and template fields')
    }
    globalConfigManager.save(payload.config)
  })

  ipcMain.handle('globalConfig:exists', () => {
    return globalConfigManager.exists()
  })

  // Shared helper: resolve effective config (merged global + repo)
  async function resolveEffectiveConfig(repoRoot?: string): Promise<ResolvedConfig | null> {
    const global = globalConfigManager.load()
    let repo: RepoConfig | null = null
    if (repoRoot) {
      const result = await repoConfigManager.load(repoRoot)
      repo = result.unwrapOr(null)
    }
    return mergeConfigs(global, repo)
  }

  ipcMain.handle('tracker:resolvedConfig', async (_event, payload: { repoRoot?: string }) => {
    return resolveEffectiveConfig(payload.repoRoot)
  })

  // --- Keychain ---

  ipcMain.handle(
    'keychain:hasCredentials',
    (_event, payload: { provider: string; baseUrl: string }) => {
      return keychainTokenStore.hasCredentials(payload.provider, payload.baseUrl)
    },
  )

  ipcMain.handle(
    'keychain:setCredentials',
    (_event, payload: { provider: string; baseUrl: string; token: string; username?: string }) => {
      if (!payload.provider || !payload.baseUrl) {
        throw new Error('Provider and baseUrl are required')
      }
      keychainTokenStore.setCredentials(
        payload.provider,
        payload.baseUrl,
        payload.token,
        payload.username,
      )
    },
  )

  ipcMain.handle(
    'keychain:deleteCredentials',
    (_event, payload: { provider: string; baseUrl: string }) => {
      keychainTokenStore.deleteCredentials(payload.provider, payload.baseUrl)
    },
  )

  ipcMain.handle(
    'keychain:getCredentials',
    (_event, payload: { provider: string; baseUrl: string }) => {
      const creds = keychainTokenStore.getCredentials(payload.provider, payload.baseUrl)
      if (!creds) return null
      // Never send token to renderer — only username and hasToken flag
      return { username: creds.username, hasToken: true }
    },
  )

  // --- Task Tracker ---

  ipcMain.handle('taskTracker:getConnections', () => {
    return taskTrackerManager.getConnections().map((c) => ({
      id: c.id,
      provider: c.provider,
      name: c.name,
      baseUrl: c.baseUrl,
      projectKey: c.projectKey,
      boardId: c.boardId,
      username: c.username,
    }))
  })

  ipcMain.handle(
    'taskTracker:addConnection',
    async (
      _event,
      payload: {
        provider: TaskTrackerProvider
        name: string
        baseUrl: string
        projectKey?: string
        boardId?: string
        username?: string
        token: string
      },
    ) => {
      if (payload.baseUrl) {
        const parsed = new URL(payload.baseUrl)
        if (!['http:', 'https:'].includes(parsed.protocol)) {
          throw new Error('Base URL must use http:// or https://')
        }
      }
      const { token, projectKey, ...rest } = payload
      const c = taskTrackerManager.addConnection({ ...rest, projectKey: projectKey ?? '' }, token)
      return {
        id: c.id,
        provider: c.provider,
        name: c.name,
        baseUrl: c.baseUrl,
        projectKey: c.projectKey,
        boardId: c.boardId,
        username: c.username,
      }
    },
  )

  ipcMain.handle('taskTracker:removeConnection', (_event, payload: { connectionId: string }) => {
    taskTrackerManager.removeConnection(payload.connectionId)
  })

  ipcMain.handle(
    'taskTracker:updateConnection',
    (
      _event,
      payload: {
        connectionId: string
        name?: string
        baseUrl?: string
        projectKey?: string
        username?: string
        token?: string
      },
    ) => {
      if (payload.baseUrl) {
        const parsed = new URL(payload.baseUrl)
        if (!['http:', 'https:'].includes(parsed.protocol)) {
          throw new Error('Base URL must use http:// or https://')
        }
      }
      const { connectionId, token, ...updates } = payload
      const c = taskTrackerManager.updateConnection(connectionId, updates, token)
      if (!c) return null
      return {
        id: c.id,
        provider: c.provider,
        name: c.name,
        baseUrl: c.baseUrl,
        projectKey: c.projectKey,
        boardId: c.boardId,
        username: c.username,
      }
    },
  )

  ipcMain.handle(
    'taskTracker:testConnection',
    async (_event, payload: { connectionId: string }) => {
      const result = await taskTrackerManager.testConnection(payload.connectionId)
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:testNewConnection',
    async (
      _event,
      payload: {
        provider: TaskTrackerProvider
        name: string
        baseUrl: string
        projectKey?: string
        boardId?: string
        username?: string
        token: string
      },
    ) => {
      if (payload.baseUrl) {
        const parsed = new URL(payload.baseUrl)
        if (!['http:', 'https:'].includes(parsed.protocol)) {
          throw new Error('Base URL must use http:// or https://')
        }
      }
      const { token, projectKey, ...rest } = payload
      const result = await taskTrackerManager.testNewConnection(
        { ...rest, projectKey: projectKey ?? '' },
        token,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:fetchBoards',
    async (_event, payload: { connectionId: string; repoRoot?: string }) => {
      const result = await taskTrackerManager.fetchBoards(payload.connectionId, payload.repoRoot)
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:fetchBoardsForNew',
    async (
      _event,
      payload: {
        provider: TaskTrackerProvider
        name: string
        baseUrl: string
        projectKey?: string
        username?: string
        token: string
      },
    ) => {
      const parsed = new URL(payload.baseUrl)
      if (!['http:', 'https:'].includes(parsed.protocol)) {
        throw new Error('Base URL must use http:// or https://')
      }
      const { token, ...connectionData } = payload
      const result = await taskTrackerManager.fetchBoardsForNew(
        { ...connectionData, projectKey: connectionData.projectKey ?? '' },
        token,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:fetchStatuses',
    async (_event, payload: { connectionId: string; boardId?: string; repoRoot?: string }) => {
      const result = await taskTrackerManager.fetchStatuses(
        payload.connectionId,
        payload.boardId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:fetchTasks',
    async (
      _event,
      payload: {
        connectionId: string
        statuses?: string[]
        assignedToMe?: boolean
        boardId?: string
        repoRoot?: string
      },
    ) => {
      const { connectionId, repoRoot, ...params } = payload
      const result = await taskTrackerManager.fetchTasks(connectionId, params, repoRoot)
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:getCurrentUser',
    async (_event, payload: { connectionId: string }) => {
      const result = await taskTrackerManager.getCurrentUserDisplayName(payload.connectionId)
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:getCurrentSprint',
    async (_event, payload: { connectionId: string; boardId?: string; repoRoot?: string }) => {
      const result = await taskTrackerManager.getCurrentSprint(
        payload.connectionId,
        payload.boardId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  // --- Config-based task tracker methods (use resolved global+repo config) ---

  ipcMain.handle(
    'trackerConfig:fetchBoards',
    async (_event, payload: { repoRoot?: string; trackerId?: string }) => {
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      if (!resolved) throw new Error('No tracker configured')
      const result = await taskTrackerManager.fetchBoardsFromConfig(
        resolved.config,
        payload.trackerId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'trackerConfig:fetchStatuses',
    async (_event, payload: { repoRoot?: string; trackerId?: string; boardId?: string }) => {
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      if (!resolved) throw new Error('No tracker configured')
      const result = await taskTrackerManager.fetchStatusesFromConfig(
        resolved.config,
        payload.boardId,
        payload.trackerId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'trackerConfig:fetchTasks',
    async (
      _event,
      payload: {
        repoRoot?: string
        trackerId?: string
        statuses?: string[]
        assignedToMe?: boolean
        boardId?: string
      },
    ) => {
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      if (!resolved) throw new Error('No tracker configured')
      const result = await taskTrackerManager.fetchTasksFromConfig(
        resolved.config,
        {
          statuses: payload.statuses,
          assignedToMe: payload.assignedToMe,
          boardId: payload.boardId,
        },
        payload.trackerId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'trackerConfig:getCurrentUser',
    async (_event, payload: { repoRoot?: string; trackerId?: string }) => {
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      if (!resolved) throw new Error('No tracker configured')
      const result = await taskTrackerManager.getCurrentUserFromConfig(
        resolved.config,
        payload.trackerId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  const TASK_KEY_RE = /^[A-Za-z0-9_#-]+-?\d+$/

  ipcMain.handle(
    'trackerConfig:fetchTaskComments',
    async (_event, payload: { repoRoot?: string; trackerId?: string; taskKey: string }) => {
      if (!TASK_KEY_RE.test(payload.taskKey)) throw new Error('Invalid task key')
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      if (!resolved) throw new Error('No tracker configured')
      const result = await taskTrackerManager.fetchTaskCommentsFromConfig(
        resolved.config,
        payload.taskKey,
        payload.trackerId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'trackerConfig:fetchTaskAttachments',
    async (_event, payload: { repoRoot?: string; trackerId?: string; taskKey: string }) => {
      if (!TASK_KEY_RE.test(payload.taskKey)) throw new Error('Invalid task key')
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      if (!resolved) throw new Error('No tracker configured')
      const result = await taskTrackerManager.fetchTaskAttachmentsFromConfig(
        resolved.config,
        payload.taskKey,
        payload.trackerId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'trackerConfig:downloadAttachment',
    async (
      _event,
      payload: { repoRoot?: string; trackerId?: string; url: string; filename: string },
    ) => {
      if (!payload.url || !/^https?:\/\//.test(payload.url)) throw new Error('Invalid URL')
      if (!payload.filename || /[\0/\\]/.test(payload.filename)) throw new Error('Invalid filename')
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      if (!resolved) throw new Error('No tracker configured')
      const result = await taskTrackerManager.downloadAttachmentFromConfig(
        resolved.config,
        payload.url,
        payload.filename,
        payload.trackerId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'trackerConfig:findTaskByKey',
    async (_event, payload: { repoRoot?: string; trackerId?: string; taskKey: string }) => {
      if (!TASK_KEY_RE.test(payload.taskKey)) throw new Error('Invalid task key')
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      if (!resolved) throw new Error('No tracker configured')
      const result = await taskTrackerManager.findTaskByKeyFromConfig(
        resolved.config,
        payload.taskKey,
        payload.trackerId,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:fetchTaskComments',
    async (_event, payload: { connectionId: string; taskKey: string; repoRoot?: string }) => {
      if (!TASK_KEY_RE.test(payload.taskKey)) throw new Error('Invalid task key')
      const result = await taskTrackerManager.fetchTaskComments(
        payload.connectionId,
        payload.taskKey,
        payload.repoRoot,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:fetchTaskAttachments',
    async (_event, payload: { connectionId: string; taskKey: string }) => {
      if (!TASK_KEY_RE.test(payload.taskKey)) throw new Error('Invalid task key')
      const result = await taskTrackerManager.fetchTaskAttachments(
        payload.connectionId,
        payload.taskKey,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:downloadAttachment',
    async (_event, payload: { connectionId: string; url: string; filename: string }) => {
      if (!payload.url || typeof payload.url !== 'string' || !/^https?:\/\//.test(payload.url)) {
        throw new Error('Invalid URL')
      }
      if (!payload.filename || /[\0/\\]/.test(payload.filename)) {
        throw new Error('Invalid filename')
      }
      const result = await taskTrackerManager.downloadAttachment(
        payload.connectionId,
        payload.url,
        payload.filename,
      )
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle('taskTracker:cleanupAttachments', (_event, payload: { filePaths: string[] }) => {
    if (!Array.isArray(payload.filePaths)) throw new Error('Invalid filePaths')
    for (const fp of payload.filePaths) {
      if (typeof fp !== 'string') continue
      taskTrackerManager.cleanupAttachmentDir(fp)
    }
  })

  ipcMain.handle(
    'taskTracker:resolveBranchName',
    async (
      _event,
      payload: {
        connectionId: string
        task: TrackerTask
        boardId?: string
        branchType?: string
        repoRoot?: string
      },
    ) => {
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      const branchTpl = resolved
        ? getBranchTemplate(resolved.config, payload.boardId)
        : { template: '{taskKey}', customVars: {} }

      // Get sprint: prefer config-based, fall back to legacy
      const sprint = resolved
        ? await taskTrackerManager
            .getCurrentSprintFromConfig(resolved.config, payload.boardId, payload.repoRoot)
            .unwrapOr(null)
        : await taskTrackerManager
            .getCurrentSprint(payload.connectionId, payload.boardId)
            .unwrapOr(null)

      const variables = buildVariables(
        payload.task,
        sprint,
        branchTpl.customVars,
        payload.branchType,
      )
      return renderBranchName(branchTpl.template, variables)
    },
  )

  ipcMain.handle(
    'taskTracker:renderBranchPreview',
    (_event, payload: { template: string; customVars?: Record<string, string> }) => {
      return renderPreview(payload.template, payload.customVars)
    },
  )

  ipcMain.handle(
    'taskTracker:getAvailablePlaceholders',
    (_event, payload?: { customVars?: Record<string, string> }) => {
      return getAvailablePlaceholders(payload?.customVars)
    },
  )

  ipcMain.handle('taskTracker:validateTemplate', (_event, payload: { template: string }) => {
    return validateTemplate(payload.template)
  })

  ipcMain.handle(
    'taskTracker:resolveBranchType',
    async (
      _event,
      payload: {
        taskType: string
        connectionId?: string
        boardId?: string
        repoRoot?: string
      },
    ) => {
      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      let typeMapping: Record<string, string> | undefined
      let hasBranchType = false

      if (resolved) {
        const branchTpl = getBranchTemplate(resolved.config, payload.boardId)
        hasBranchType = branchTpl.template.includes('{branchType}')
        typeMapping = branchTpl.typeMapping
      }

      return {
        defaultType: resolveBranchType(payload.taskType, typeMapping),
        options: BRANCH_TYPE_OPTIONS,
        hasBranchType,
      }
    },
  )

  ipcMain.handle('taskTracker:findTaskByKey', async (_event, payload: { taskKey: string }) => {
    return taskTrackerManager.findTaskByKey(payload.taskKey)
  })

  ipcMain.handle(
    'taskTracker:resolvePRPreview',
    async (
      _event,
      payload: {
        taskKey: string
        connectionId?: string
        boardId?: string
        repoRoot?: string
      },
    ) => {
      let task: TrackerTask | null = null
      if (payload.taskKey) {
        task = await taskTrackerManager.findTaskByKey(payload.taskKey).catch(() => null)
      }

      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      const prTpl = resolved
        ? getPRTemplate(resolved.config, payload.boardId)
        : {
            titleTemplate: '[{taskKey}] {taskTitle}',
            bodyTemplate: '## {taskKey}: {taskTitle}\n\n{taskUrl}',
            defaultTargetBranch: '',
            targetRules: [],
          }

      const titleTemplate = prTpl.titleTemplate
      const defaultBranch = prTpl.defaultTargetBranch || 'develop'

      const title = titleTemplate
        .replace(/\{taskKey\}/g, task?.key ?? payload.taskKey)
        .replace(/\{taskTitle\}/g, task?.summary ?? '')
        .replace(/\{taskType\}/g, task?.type ?? '')
        .replace(/\{boardKey\}/g, (task?.key ?? payload.taskKey).split('-')[0] ?? '')

      return { title, targetBranch: defaultBranch }
    },
  )

  ipcMain.handle(
    'taskTracker:createPR',
    async (
      _event,
      payload: {
        repoRoot: string
        task: TrackerTask
        sourceBranch: string
        connectionId?: string
        boardId?: string
      },
    ) => {
      let task = payload.task
      if (task.key && !task.summary) {
        const found = await taskTrackerManager.findTaskByKey(task.key)
        if (found) task = found
      }

      const resolved = await resolveEffectiveConfig(payload.repoRoot)
      const prTpl = resolved
        ? getPRTemplate(resolved.config, payload.boardId)
        : {
            titleTemplate: '[{taskKey}] {taskTitle}',
            bodyTemplate: '## {taskKey}: {taskTitle}\n\n{taskUrl}',
            defaultTargetBranch: 'develop',
            targetRules: [] as Array<{ taskType: string; targetPattern: string }>,
          }

      const branchResult = await GitRepository.listBranches(payload.repoRoot)
      const branches = unwrapOrThrow(branchResult, gitErrorMessage)
      const existingBranches = [...branches.local, ...branches.remote]

      const prConfig = buildPRConfig(
        prTpl.titleTemplate,
        prTpl.bodyTemplate,
        prTpl.defaultTargetBranch || 'develop',
        prTpl.targetRules,
      )
      const result = await createPullRequest({
        repoRoot: payload.repoRoot,
        task,
        sourceBranch: payload.sourceBranch,
        prConfig,
        existingBranches,
      })
      return unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle(
    'taskTracker:findPR',
    async (_event, payload: { repoRoot: string; branch: string }) => {
      try {
        const { stdout } = await execFileAsync(
          'gh',
          ['pr', 'view', payload.branch, '--json', 'url', '--jq', '.url'],
          { cwd: payload.repoRoot },
        )
        return stdout.trim() || null
      } catch {
        return null
      }
    },
  )

  // --- Worktree Setup ---

  const setupAbortControllers = new Map<number, AbortController>()

  ipcMain.handle(
    'worktree:runSetup',
    async (event, payload: { workspaceId: string; repoRoot: string; newWorktreePath: string }) => {
      const configJson = preferencesStore.get(`workspace:${payload.workspaceId}:worktreeSetup`)
      if (!configJson) return { success: true, errors: [] }

      let actions: WorktreeSetupAction[]
      try {
        actions = JSON.parse(configJson) as WorktreeSetupAction[]
      } catch {
        return { success: false, errors: ['Invalid worktree setup config'] }
      }

      if (actions.length === 0) return { success: true, errors: [] }

      const worktrees = await GitRepository.listWorktrees(payload.repoRoot).unwrapOr([])
      const mainWorktree = worktrees.find((wt) => wt.isMain)
      const mainWorktreePath = mainWorktree?.path ?? payload.repoRoot

      const sender = event.sender
      const controller = new AbortController()
      setupAbortControllers.set(sender.id, controller)

      try {
        return await runWorktreeSetup(
          actions,
          {
            repoRoot: payload.repoRoot,
            mainWorktreePath,
            newWorktreePath: payload.newWorktreePath,
          },
          (progress) => {
            if (!sender.isDestroyed()) {
              sender.send('worktree:setupProgress', progress)
            }
          },
          controller.signal,
        )
      } finally {
        setupAbortControllers.delete(sender.id)
      }
    },
  )

  ipcMain.on('worktree:abortSetup', (event) => {
    const controller = setupAbortControllers.get(event.sender.id)
    controller?.abort()
  })

  // --- Onboarding ---

  ipcMain.handle('onboarding:getCompleted', () => {
    return onboardingStore.getCompleted()
  })

  ipcMain.handle(
    'onboarding:complete',
    (_event, payload: { stepIds: string[]; appVersion: string }) => {
      if (!Array.isArray(payload.stepIds) || typeof payload.appVersion !== 'string') return
      if (payload.stepIds.length === 0 || !payload.appVersion) return
      const safeIds = payload.stepIds.filter(
        (id) => typeof id === 'string' && id.length > 0 && id.length < 100,
      )
      if (safeIds.length === 0) return
      onboardingStore.completeMany(safeIds, payload.appVersion)
    },
  )

  ipcMain.handle('onboarding:reset', () => {
    onboardingStore.reset()
  })

  // ── GitHub PR features ──────────────────────────────────────────────

  ipcMain.handle('github:fetchBranchPRs', async (_event, payload: { repoRoot: string }) => {
    const found = await gitHubService.findGitHubConnection(payload.repoRoot)
    // No connection configured — silent empty return (expected for non-GitHub repos)
    if (found.isErr() || !found.value) return {}
    const { token, repo } = found.value
    const worktrees = await GitRepository.listWorktrees(payload.repoRoot).unwrapOr([])
    const branches = worktrees.map((w) => w.branch).filter((b) => b && b !== '(detached)')
    if (branches.length === 0) return {}
    const result = await gitHubService.fetchOpenPRsForBranches(
      repo.apiUrl,
      token,
      repo.owner,
      repo.repo,
      branches,
    )
    return unwrapOrThrow(result, gitHubErrorMessage)
  })

  ipcMain.handle('github:getRepoInfo', async (_event, payload: { repoRoot: string }) => {
    const found = await gitHubService.findGitHubConnection(payload.repoRoot)
    if (found.isErr() || !found.value) return null
    const { token, repo } = found.value
    const result = await gitHubService.getRepoInfo(repo.apiUrl, token, repo.owner, repo.repo)
    return unwrapOrThrow(result, gitHubErrorMessage)
  })

  ipcMain.handle(
    'github:createPR',
    async (
      _event,
      payload: {
        repoRoot: string
        title: string
        body: string
        baseRefName: string
        draft: boolean
      },
    ) => {
      const found = await gitHubService.findGitHubConnection(payload.repoRoot)
      if (found.isErr() || !found.value) {
        throw new Error('No GitHub connection found for this repository')
      }
      const { token, repo } = found.value

      const pushResult = await GitRepository.push(payload.repoRoot)
      if (pushResult.isErr()) {
        throw new Error(`Failed to push branch: ${gitErrorMessage(pushResult.error)}`)
      }

      const repoInfo = await gitHubService.getRepoInfo(repo.apiUrl, token, repo.owner, repo.repo)
      const repoInfoValue = unwrapOrThrow(repoInfo, gitHubErrorMessage)

      const branch = await GitRepository.getBranch(payload.repoRoot).unwrapOr(null)
      if (!branch) throw new Error('Could not determine current branch')

      const result = await gitHubService.createPR(repo.apiUrl, token, {
        repositoryId: repoInfoValue.id,
        headRefName: branch,
        baseRefName: payload.baseRefName || repoInfoValue.defaultBranch,
        title: payload.title,
        body: payload.body,
        draft: payload.draft,
      })
      return unwrapOrThrow(result, gitHubErrorMessage)
    },
  )

  ipcMain.handle('github:getRepoIdentifier', async (_event, payload: { repoRoot: string }) => {
    const result = await gitHubService.getRepoIdentifier(payload.repoRoot)
    return result.unwrapOr(null)
  })

  // --- Remote control (WebRTC pairing via QR) ---

  ipcMain.handle('remote:start', async (event) => {
    if (!remoteSessionService.isEnabledInPreferences()) {
      throw new Error('Remote control is disabled in settings')
    }
    // The host webContents owns this session — peer signals are routed back
    // to this window only, not broadcast to the other windows.
    const result = await remoteSessionService.start(event.sender.id)
    return unwrapOrThrow(result, remoteServerErrorMessage)
  })

  ipcMain.handle('remote:ensureListening', async (event) => {
    // Best-effort: auto-bind the signaling server in listen mode so a
    // previously trusted phone can reconnect without the user opening the
    // Remote Connection modal. Silently no-ops if the user hasn't opted in,
    // has no trusted devices, or the network is unavailable. Swallowed
    // errors never surface to the renderer — a failed listen should not
    // block app startup or UI rendering.
    await remoteSessionService.ensureListening(event.sender.id).match(
      () => {},
      () => {},
    )
  })

  ipcMain.handle('remote:stop', async () => {
    const result = await remoteSessionService.stop()
    return unwrapOrThrow(result, remoteServerErrorMessage)
  })

  ipcMain.handle('remote:getStatus', () => {
    return remoteSessionService.getStatus()
  })

  ipcMain.handle('remote:acceptDevice', async (_event, payload: { remember: boolean }) => {
    const result = await remoteSessionService.acceptPendingDevice(payload?.remember === true)
    return unwrapOrThrow(result, remoteServerErrorMessage)
  })

  ipcMain.handle('remote:rejectDevice', async () => {
    const result = await remoteSessionService.rejectPendingDevice()
    return unwrapOrThrow(result, remoteServerErrorMessage)
  })

  ipcMain.handle('remote:sendSignal', async (event, payload: unknown) => {
    // Only the session's host window may forward signaling frames. This
    // protects against another window racing to answer an offer the peer
    // sent for an entirely different controller.
    if (event.sender.id !== remoteSessionService.currentHostWcId) {
      throw new Error('Only the session host window can forward signals')
    }
    if (typeof payload !== 'object' || payload === null) {
      throw new Error('Invalid signal payload')
    }
    const result = await remoteSessionService.forwardSignalToPeer(
      payload as Record<string, unknown>,
    )
    return unwrapOrThrow(result, remoteServerErrorMessage)
  })

  ipcMain.handle('remote:listTrustedDevices', () => {
    return remoteSessionService.listTrustedDevices()
  })

  ipcMain.handle('remote:removeTrustedDevice', (_event, payload: { deviceId: string }) => {
    if (!payload || typeof payload.deviceId !== 'string' || payload.deviceId.length === 0) {
      throw new Error('Invalid deviceId')
    }
    remoteSessionService.removeTrustedDevice(payload.deviceId)
  })

  // --- Run Configurations ---

  const runConfigInstances = new Map<string, number>()

  ipcMain.handle('runConfig:discover', async (event, payload: { repoRoot: string }) => {
    await validatePathAccess(event.sender.id, payload.repoRoot)
    const result = await runConfigManager.discover(payload.repoRoot)
    return result.unwrapOr([])
  })

  ipcMain.handle(
    'runConfig:save',
    async (event, payload: { configDir: string; config: { configurations: unknown[] } }) => {
      await validatePathAccess(event.sender.id, payload.configDir)
      const result = await runConfigManager.saveFile(
        payload.configDir,
        payload.config as import('../runConfig/types').RunConfigFile,
      )
      unwrapOrThrow(result, runConfigErrorMessage)
    },
  )

  ipcMain.handle(
    'runConfig:addConfig',
    async (
      event,
      payload: { configDir: string; configuration: import('../runConfig/types').RunConfiguration },
    ) => {
      await validatePathAccess(event.sender.id, payload.configDir)
      const result = await runConfigManager.addConfiguration(
        payload.configDir,
        payload.configuration,
      )
      unwrapOrThrow(result, runConfigErrorMessage)
    },
  )

  ipcMain.handle(
    'runConfig:updateConfig',
    async (
      event,
      payload: {
        configDir: string
        name: string
        configuration: import('../runConfig/types').RunConfiguration
      },
    ) => {
      await validatePathAccess(event.sender.id, payload.configDir)
      const result = await runConfigManager.updateConfiguration(
        payload.configDir,
        payload.name,
        payload.configuration,
      )
      unwrapOrThrow(result, runConfigErrorMessage)
    },
  )

  ipcMain.handle(
    'runConfig:deleteConfig',
    async (event, payload: { configDir: string; name: string }) => {
      await validatePathAccess(event.sender.id, payload.configDir)
      const result = await runConfigManager.deleteConfiguration(payload.configDir, payload.name)
      unwrapOrThrow(result, runConfigErrorMessage)
    },
  )

  ipcMain.handle(
    'runConfig:execute',
    async (event, payload: { configDir: string; name: string; cwd?: string }) => {
      await validatePathAccess(event.sender.id, payload.configDir)
      const fileResult = await runConfigManager.loadFile(payload.configDir)
      const file = unwrapOrThrow(fileResult, runConfigErrorMessage)
      const config = file.configurations.find((c) => c.name === payload.name)
      if (!config) throw new Error(`Configuration "${payload.name}" not found`)

      if (config.max_instances && config.max_instances > 0) {
        const current = runConfigInstances.get(`${payload.configDir}::${payload.name}`) ?? 0
        if (current >= config.max_instances) {
          throw new Error(`"${payload.name}" is already running (max ${config.max_instances})`)
        }
      }

      if (!payload.cwd) throw new Error('No worktree selected')
      await validatePathAccess(event.sender.id, payload.cwd)
      const worktreeRoot = path.resolve(payload.cwd)
      const cwd = config.cwd ? path.resolve(worktreeRoot, config.cwd) : worktreeRoot
      if (config.cwd && cwd !== worktreeRoot && !cwd.startsWith(worktreeRoot + path.sep)) {
        throw new Error('config.cwd must not escape the worktree directory')
      }
      const env = config.env
      const fullCommand = config.args ? `${config.command} ${config.args}` : config.command

      // Pre-run hook (30s timeout)
      if (config.pre_run) {
        const PRE_RUN_TIMEOUT = 30_000
        const pre = shellExecArgs(config.pre_run)
        const preSession = ptyManager.spawn({ command: pre.command, args: pre.args, cwd, env })
        let preOutput = ''
        preSession.pty.onData((data) => {
          preOutput += data
        })
        await new Promise<void>((resolve, reject) => {
          let done = false
          const timer = setTimeout(() => {
            if (!done) {
              done = true
              ptyManager.kill(preSession.id)
              reject(new Error(`pre_run "${config.pre_run}" timed out after 30s`))
            }
          }, PRE_RUN_TIMEOUT)
          preSession.pty.onExit(({ exitCode }) => {
            if (done) return
            done = true
            clearTimeout(timer)
            ptyManager.kill(preSession.id)
            if (exitCode !== 0) {
              const lastLines = preOutput.trim().split('\n').slice(-5).join('\n')
              reject(
                new Error(`pre_run "${config.pre_run}" failed (exit ${exitCode}):\n${lastLines}`),
              )
            } else resolve()
          })
        })
      }

      // Run main command through shell so PATH is resolved
      const main = shellExecArgs(fullCommand)
      const session = ptyManager.spawn({ command: main.command, args: main.args, cwd, env })
      const wsUrl = await wsBridge.create(session.id, session.pty)
      const senderId = event.sender.id
      windowManager.trackPtySession(senderId, session.id)
      const instanceKey = `${payload.configDir}::${payload.name}`
      runConfigInstances.set(instanceKey, (runConfigInstances.get(instanceKey) ?? 0) + 1)

      const sender = event.sender
      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', { sessionId: session.id, exitCode, signal })
        }
        windowManager.untrackPtySession(senderId, session.id)
        const count = (runConfigInstances.get(`${payload.configDir}::${payload.name}`) ?? 1) - 1
        if (count <= 0) runConfigInstances.delete(`${payload.configDir}::${payload.name}`)
        else runConfigInstances.set(`${payload.configDir}::${payload.name}`, count)

        // Post-run hook
        if (config.post_run) {
          const postCmd = config.post_run
          const post = shellExecArgs(postCmd)
          const postSession = ptyManager.spawn({
            command: post.command,
            args: post.args,
            cwd,
            env,
          })
          const POST_RUN_TIMEOUT = 30_000
          let postDone = false
          const postTimer = setTimeout(() => {
            if (!postDone) {
              postDone = true
              ptyManager.kill(postSession.id)
              if (!sender.isDestroyed()) {
                sender.send('runConfig:postRunResult', {
                  success: false,
                  command: postCmd,
                  exitCode: -1,
                })
              }
            }
          }, POST_RUN_TIMEOUT)
          postSession.pty.onExit(({ exitCode: postExit }) => {
            if (postDone) return
            postDone = true
            clearTimeout(postTimer)
            if (!sender.isDestroyed()) {
              sender.send(
                'runConfig:postRunResult',
                postExit === 0
                  ? { success: true, command: postCmd }
                  : { success: false, command: postCmd, exitCode: postExit },
              )
            }
            ptyManager.kill(postSession.id)
          })
        }
      })

      return { sessionId: session.id, wsUrl }
    },
  )

  // --- Skills ---

  ipcMain.handle('skills:list', (_event, payload?: SkillListOptions) => {
    return JSON.parse(JSON.stringify(skillRegistry.list(payload)))
  })

  ipcMain.handle('skills:get', (_event, payload: { id: string }) => {
    const skill = skillRegistry.get(payload.id)
    return skill ? JSON.parse(JSON.stringify(skill)) : null
  })

  ipcMain.handle('skills:install', async (_event, payload: SkillInstallOptions) => {
    const result = await skillInstaller.install(payload)
    const skill = unwrapOrThrow(result, skillErrorMessage)
    skillRegistry.refresh()
    broadcastSkillsChanged()
    return JSON.parse(JSON.stringify(skill))
  })

  ipcMain.handle(
    'skills:remove',
    async (_event, payload: { id: string; workspacePath?: string }) => {
      const result = await skillInstaller.remove(payload.id, payload.workspacePath)
      unwrapOrThrow(result, skillErrorMessage)
      skillRegistry.refresh()
      broadcastSkillsChanged()
      return { success: true }
    },
  )

  ipcMain.handle(
    'skills:update',
    async (_event, payload: { id: string; workspacePath?: string }) => {
      const result = await skillInstaller.update(payload.id, payload.workspacePath)
      const skill = unwrapOrThrow(result, skillErrorMessage)
      skillRegistry.refresh()
      broadcastSkillsChanged()
      return JSON.parse(JSON.stringify(skill))
    },
  )

  ipcMain.handle(
    'skills:toggleAgent',
    async (
      _event,
      payload: { id: string; agent: string; enabled: boolean; workspacePath?: string },
    ) => {
      const skill = unwrapOrThrow(
        skillRegistry.get(payload.id)
          ? ok(skillRegistry.get(payload.id)!)
          : err({ _tag: 'SkillNotFound' as const, skillId: payload.id } as SkillError),
        skillErrorMessage,
      )
      const enabledAgents: SkillAgentTarget[] = payload.enabled
        ? ([...new Set([...skill.enabledAgents, payload.agent])] as SkillAgentTarget[])
        : skill.enabledAgents.filter((a) => a !== payload.agent)

      // Deploy or undeploy files BEFORE updating DB
      // Global skills use transformer's globalDir(); project skills need workspacePath
      const transformer = getTransformer(payload.agent as SkillAgentTarget)
      if (transformer) {
        if (skill.scope === 'project' && !payload.workspacePath) {
          unwrapOrThrow(
            err({
              _tag: 'InstallFailed',
              skillId: payload.id,
              reason: 'workspacePath is required for project-scoped skill agent toggle',
            } as SkillError),
            skillErrorMessage,
          )
        }
        // Pass empty string for global — transformers check scope and use globalDir()
        const targetRoot = payload.workspacePath ?? ''
        const skillForDeploy = { ...skill, enabledAgents }
        if (payload.enabled) {
          const deployResult = await transformer.deploy(skillForDeploy, targetRoot)
          unwrapOrThrow(deployResult, skillErrorMessage)
        } else {
          const undeployResult = await transformer.undeploy(skillForDeploy, targetRoot)
          unwrapOrThrow(undeployResult, skillErrorMessage)
        }
      }

      // Update DB only after successful deploy/undeploy
      skillStore.updateEnabledAgents(payload.id, enabledAgents)
      skillRegistry.refresh()
      broadcastSkillsChanged()
      return { success: true }
    },
  )

  ipcMain.handle('skills:scan', async (_event, payload?: { workspacePath?: string }) => {
    const results = await scanSkills(payload?.workspacePath)
    return JSON.parse(JSON.stringify(results))
  })

  ipcMain.handle('skills:deleteFile', async (_event, payload: { filePath: string }) => {
    const filePath = path.normalize(path.resolve(payload.filePath))
    const ext = path.extname(filePath).toLowerCase()
    if (!['.md', '.mdc', '.yaml', '.yml'].includes(ext)) {
      unwrapOrThrow(
        err({
          _tag: 'InvalidSource',
          source: payload.filePath,
          reason: 'Can only delete skill files (.md, .mdc, .yaml, .yml)',
        } as SkillError),
        skillErrorMessage,
      )
    }
    const skillDirPatterns = [
      /[/\\]\.claude[/\\](commands|skills)[/\\]/,
      /[/\\]\.gemini[/\\]skills[/\\]/,
      /[/\\]\.cursor[/\\]rules[/\\]/,
      /[/\\]\.opencode[/\\]skills[/\\]/,
      /[/\\]\.agents[/\\]skills[/\\]/,
      /[/\\]\.claude[/\\]plugins[/\\]cache[/\\]/,
    ]
    if (!skillDirPatterns.some((p) => p.test(filePath))) {
      unwrapOrThrow(
        err({
          _tag: 'InvalidSource',
          source: payload.filePath,
          reason: 'Can only delete files within agent skill directories',
        } as SkillError),
        skillErrorMessage,
      )
    }
    await fs.promises.unlink(filePath)
    return { success: true }
  })
}
