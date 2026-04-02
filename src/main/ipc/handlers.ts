import { ipcMain, dialog, shell, BrowserWindow, systemPreferences } from 'electron'
import os from 'os'
import fs from 'fs'
import path from 'path'
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
import type { CredentialStore } from '../db/CredentialStore'
import { TmuxManager } from '../pty/TmuxManager'
import { execFile } from 'child_process'
import { promisify } from 'util'
import { GitRepository } from '../git/GitRepository'
import { GitWatcher, type GitRefreshFlags } from '../git/GitWatcher'
import { runWorktreeSetup } from '../worktree/WorktreeSetupRunner'

const execFileAsync = promisify(execFile)
import type { WorktreeSetupAction } from '../db/types'
import { generateCommitMessage } from '../ai/commitMessageGenerator'
import type { TaskTrackerManager } from '../taskTracker/TaskTrackerManager'
import type { TaskTrackerProvider, TrackerTask } from '../taskTracker/types'
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
): void {
  function broadcastToolsChanged(): void {
    const tools = toolRegistry.getAll()
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.send('tools:changed', tools)
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
      ptyManager.resize(payload.sessionId, payload.cols, payload.rows)
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

        // Parse settings.json overrides from prefs
        let settingsOverrides: Record<string, unknown> | undefined
        const settingsJsonRaw = preferencesStore.get(`${tool.id}.settingsJson`)
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
        args.push(...agentSessionManager.getCliArgs(tool.id, preferencesStore))
        env = {
          CANOPY_HOOK_PORT: String(agentSession.hookPort),
          CANOPY_HOOK_TOKEN: agentSession.hookAuthToken,
          ...agentSession.settingsEnv,
          ...agentSessionManager.getEnvVars(tool.id, preferencesStore),
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
  })

  ipcMain.handle('db:prefs:getAll', () => {
    return preferencesStore.getAll()
  })

  ipcMain.handle('db:prefs:delete', (_event, payload: { key: string }) => {
    preferencesStore.delete(payload.key)
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

  // --- App / Shell ---

  ipcMain.handle('app:homedir', () => os.homedir())

  ipcMain.handle('app:showInFolder', (_event, payload: { path: string }) => {
    shell.showItemInFolder(payload.path)
  })

  // --- App: Multi-window ---

  ipcMain.handle('app:newWindow', () => {
    windowManager.createWindow()
  })

  ipcMain.handle('app:setWorkspacePath', (event, payload: { path: string }) => {
    windowManager.addWorkspacePath(event.sender.id, payload.path)
    const configs = windowManager.getAllWindowConfigs()
    if (configs.length > 0) {
      preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
    } else {
      preferencesStore.delete('openWindowConfigs')
    }
  })

  ipcMain.handle('app:setActiveWorktree', (event, payload: { path: string }) => {
    windowManager.setActiveWorktree(event.sender.id, payload.path)
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
    const configs = windowManager.getAllWindowConfigs()
    if (configs.length > 0) {
      preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
    } else {
      preferencesStore.delete('openWindowConfigs')
    }
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

  ipcMain.handle('dialog:openFolder', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (!win || win.isDestroyed()) return null
    const result = await dialog.showOpenDialog(win, {
      properties: ['openDirectory', 'createDirectory'],
    })
    return result.canceled ? null : result.filePaths[0]
  })

  // --- Git ---

  ipcMain.handle('git:detect', async (_event, payload: { path: string }) => {
    return GitRepository.detect(payload.path)
  })

  ipcMain.handle('git:worktrees', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.listWorktrees(payload.repoRoot)
  })

  ipcMain.handle('git:status', async (_event, payload: { path: string }) => {
    const [branch, isDirty, aheadBehind] = await Promise.all([
      GitRepository.getBranch(payload.path),
      GitRepository.isDirty(payload.path),
      GitRepository.getAheadBehind(payload.path),
    ])
    return { branch, isDirty, aheadBehind }
  })

  ipcMain.handle(
    'git:watch',
    async (
      event,
      payload: {
        repoRoot: string
        snapshot?: {
          isGitRepo: boolean
          repoRoot: string | null
          branch: string | null
          worktrees: {
            path: string
            head: string
            branch: string
            isMain: boolean
            isBare: boolean
          }[]
          isDirty: boolean
          aheadBehind: { ahead: number; behind: number } | null
        }
      },
    ) => {
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
      watcher.start()
      windowManager.setGitWatcher(senderId, payload.repoRoot, watcher)
    },
  )

  ipcMain.handle('git:unwatch', (event, payload?: { repoRoot?: string }) => {
    if (payload?.repoRoot) {
      windowManager.disposeGitWatcher(event.sender.id, payload.repoRoot)
    } else {
      windowManager.disposeAllGitWatchers(event.sender.id)
    }
  })

  ipcMain.handle('git:init', async (_event, payload: { path: string }) => {
    await execFileAsync('git', ['init'], { cwd: payload.path })
    return GitRepository.detect(payload.path)
  })

  // --- Workspace Git Status Refresh ---

  ipcMain.handle(
    'db:workspace:refreshGitStatus',
    async (_event, payload: { id: string; path: string }) => {
      const info = await GitRepository.detect(payload.path)
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
      return GitRepository.commit(payload.repoRoot, payload.message, payload.stageAll)
    },
  )

  ipcMain.handle('git:push', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.push(payload.repoRoot)
  })

  ipcMain.handle('git:pull', async (_event, payload: { repoRoot: string; rebase: boolean }) => {
    return GitRepository.pull(payload.repoRoot, payload.rebase)
  })

  ipcMain.handle('git:fetch', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.fetch(payload.repoRoot)
  })

  ipcMain.handle('git:fetchAll', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.fetchAll(payload.repoRoot)
  })

  ipcMain.handle('git:stash', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.stash(payload.repoRoot)
  })

  ipcMain.handle('git:stashPop', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.stashPop(payload.repoRoot)
  })

  ipcMain.handle('git:branches', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.listBranches(payload.repoRoot)
  })

  ipcMain.handle(
    'git:branchCreate',
    async (_event, payload: { repoRoot: string; name: string; baseBranch: string }) => {
      return GitRepository.createBranch(payload.repoRoot, payload.name, payload.baseBranch)
    },
  )

  ipcMain.handle('git:checkout', async (_event, payload: { repoRoot: string; branch: string }) => {
    return GitRepository.checkout(payload.repoRoot, payload.branch)
  })

  ipcMain.handle(
    'git:branchDelete',
    async (_event, payload: { repoRoot: string; name: string; force: boolean }) => {
      return GitRepository.deleteBranch(payload.repoRoot, payload.name, payload.force)
    },
  )

  ipcMain.handle(
    'git:branchDeleteRemote',
    async (_event, payload: { repoRoot: string; remote: string; name: string }) => {
      return GitRepository.deleteRemoteBranch(payload.repoRoot, payload.remote, payload.name)
    },
  )

  ipcMain.handle('git:pushInfo', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.getPushInfo(payload.repoRoot)
  })

  ipcMain.handle(
    'git:branchMerged',
    async (_event, payload: { repoRoot: string; branch: string }) => {
      return GitRepository.isBranchMerged(payload.repoRoot, payload.branch)
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
      return GitRepository.worktreeAdd(
        payload.repoRoot,
        resolvedPath,
        payload.branch,
        payload.baseBranch,
      )
    },
  )

  ipcMain.handle(
    'git:worktreeRemove',
    async (_event, payload: { repoRoot: string; path: string; force: boolean }) => {
      return GitRepository.worktreeRemove(payload.repoRoot, payload.path, payload.force)
    },
  )

  ipcMain.handle(
    'git:unmergedCommits',
    async (_event, payload: { repoRoot: string; branch: string }) => {
      return GitRepository.getUnmergedCommits(payload.repoRoot, payload.branch)
    },
  )

  ipcMain.handle(
    'git:statusPorcelain',
    async (_event, payload: { repoRoot: string; worktreePath?: string }) => {
      return GitRepository.getStatusPorcelain(payload.repoRoot, payload.worktreePath)
    },
  )

  ipcMain.handle('git:generateCommitMessage', async (_event, payload: { repoRoot: string }) => {
    const diff = await GitRepository.getDiff(payload.repoRoot)
    if (!diff.trim()) return null
    return generateCommitMessage(diff, preferencesStore)
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
    },
  )

  ipcMain.handle('credentials:delete', (_event, payload: { id: string }) => {
    credentialStore.delete(payload.id)
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
    async (event, payload: { id: string; domain: string }) => {
      // Require system authentication before revealing passwords
      if (process.platform === 'darwin') {
        try {
          await systemPreferences.promptTouchID('reveal a saved password')
        } catch {
          return null // User cancelled or auth failed
        }
      } else if (process.platform === 'win32') {
        // Windows: native credential prompt via PowerShell
        try {
          const ps = `
            Add-Type -AssemblyName System.Runtime.WindowsRuntime
            $null = [Windows.Security.Credentials.UI.UserConsentVerifier,Windows.Security.Credentials.UI,ContentType=WindowsRuntime]
            $result = [Windows.Security.Credentials.UI.UserConsentVerifier]::RequestVerificationAsync('Canopy wants to reveal a saved password').GetAwaiter().GetResult()
            if ($result -ne 'Verified') { exit 1 }
          `
          await execFileAsync('powershell', ['-NoProfile', '-Command', ps])
        } catch {
          return null
        }
      } else {
        // Linux: confirmation dialog (zenity/kdialog not guaranteed)
        const win = BrowserWindow.fromWebContents(event.sender) ?? BrowserWindow.getFocusedWindow()
        if (!win) return null
        const { response } = await dialog.showMessageBox(win, {
          type: 'warning',
          buttons: ['Reveal Password', 'Cancel'],
          defaultId: 1,
          cancelId: 1,
          title: 'Authentication Required',
          message: 'Reveal saved password?',
          detail: `You are about to reveal the password for ${payload.domain}. Make sure no one is looking at your screen.`,
        })
        if (response !== 0) return null
      }
      return credentialStore.getForDomain(payload.domain).find((c) => c.id === payload.id) ?? null
    },
  )

  // --- Filesystem ---

  const IGNORED_NAMES = new Set([
    '.git',
    'node_modules',
    '.next',
    '__pycache__',
    '.DS_Store',
    '.svelte-kit',
    '.turbo',
    '.nuxt',
    '.output',
  ])

  async function validatePathAccess(wcId: number, targetPath: string): Promise<void> {
    const resolved = await fs.promises.realpath(targetPath)
    const allowed = windowManager.getWorkspacePaths(wcId)
    const ok = allowed.some((wp) => resolved === wp || resolved.startsWith(wp + path.sep))
    if (!ok) throw new Error('Access denied: path outside workspace')
  }

  ipcMain.handle('fs:readDir', async (event, payload: { dirPath: string }) => {
    await validatePathAccess(event.sender.id, payload.dirPath)
    const entries = await fs.promises.readdir(payload.dirPath, { withFileTypes: true })
    const filtered = entries.filter((e) => {
      if (IGNORED_NAMES.has(e.name)) return false
      if (e.name.startsWith('.') && e.name !== '.env.example') return false
      return true
    })
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
    const stat = await fs.promises.stat(payload.filePath)
    const size = stat.size

    const fd = await fs.promises.open(payload.filePath, 'r')
    try {
      const readSize = Math.min(size, maxBytes)
      const buf = Buffer.alloc(readSize)
      await fd.read(buf, 0, readSize, 0)

      // Binary detection: check first 8KB for null bytes
      const detectEnd = Math.min(readSize, 8192)
      for (let i = 0; i < detectEnd; i++) {
        if (buf[i] === 0) return { binary: true, size }
      }

      return {
        content: buf.toString('utf-8'),
        truncated: size > maxBytes,
        size,
        binary: false,
      }
    } finally {
      await fd.close()
    }
  })

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
        projectKey: string
        boardId?: string
        username?: string
        token: string
      },
    ) => {
      const parsed = new URL(payload.baseUrl)
      if (!['http:', 'https:'].includes(parsed.protocol)) {
        throw new Error('Base URL must use http:// or https://')
      }
      const { token, ...connectionData } = payload
      const c = taskTrackerManager.addConnection(connectionData, token)
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
      return taskTrackerManager.testConnection(payload.connectionId)
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
        projectKey: string
        boardId?: string
        username?: string
        token: string
      },
    ) => {
      const parsed = new URL(payload.baseUrl)
      if (!['http:', 'https:'].includes(parsed.protocol)) {
        throw new Error('Base URL must use http:// or https://')
      }
      const { token, ...connectionData } = payload
      return taskTrackerManager.testNewConnection(connectionData, token)
    },
  )

  ipcMain.handle('taskTracker:fetchBoards', async (_event, payload: { connectionId: string }) => {
    return taskTrackerManager.fetchBoards(payload.connectionId)
  })

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
      return taskTrackerManager.fetchBoardsForNew(
        { ...connectionData, projectKey: connectionData.projectKey ?? '' },
        token,
      )
    },
  )

  ipcMain.handle(
    'taskTracker:fetchStatuses',
    async (_event, payload: { connectionId: string; boardId?: string }) => {
      return taskTrackerManager.fetchStatuses(payload.connectionId, payload.boardId)
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
      },
    ) => {
      const { connectionId, ...params } = payload
      return taskTrackerManager.fetchTasks(connectionId, params)
    },
  )

  ipcMain.handle(
    'taskTracker:getCurrentUser',
    async (_event, payload: { connectionId: string }) => {
      return taskTrackerManager.getCurrentUserDisplayName(payload.connectionId)
    },
  )

  ipcMain.handle(
    'taskTracker:getCurrentSprint',
    async (_event, payload: { connectionId: string; boardId?: string }) => {
      return taskTrackerManager.getCurrentSprint(payload.connectionId, payload.boardId)
    },
  )

  ipcMain.handle(
    'taskTracker:resolveBranchName',
    async (
      _event,
      payload: {
        connectionId: string
        task: TrackerTask
        boardId?: string
        branchType?: string
      },
    ) => {
      // Resolve template: board → connection → global
      let template = '{taskKey}'
      let customVars: Record<string, string> = {}

      const keys = [
        payload.boardId && `taskTracker.branchTemplate.${payload.connectionId}.${payload.boardId}`,
        `taskTracker.branchTemplate.${payload.connectionId}`,
        'taskTracker.branchTemplate',
      ].filter(Boolean) as string[]

      for (const key of keys) {
        const raw = preferencesStore.get(key)
        if (raw) {
          try {
            const config = JSON.parse(raw)
            if (config.template) {
              template = config.template
              customVars = config.customVars ?? {}
              break
            }
          } catch {
            // try next level
          }
        }
      }

      // Get sprint: from task data or from API
      const sprint = await taskTrackerManager
        .getCurrentSprint(payload.connectionId, payload.boardId)
        .catch(() => null)

      const variables = buildVariables(payload.task, sprint, customVars, payload.branchType)
      return renderBranchName(template, variables)
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
    (_event, payload: { taskType: string; connectionId?: string; boardId?: string }) => {
      const typeMappingJson = preferencesStore.get('taskTracker.typeMapping')
      let typeMapping: Record<string, string> | undefined
      if (typeMappingJson) {
        try {
          typeMapping = JSON.parse(typeMappingJson)
        } catch {
          // use defaults
        }
      }

      // Check if resolved template contains {branchType}
      const keys = [
        payload.boardId &&
          payload.connectionId &&
          `taskTracker.branchTemplate.${payload.connectionId}.${payload.boardId}`,
        payload.connectionId && `taskTracker.branchTemplate.${payload.connectionId}`,
        'taskTracker.branchTemplate',
      ].filter(Boolean) as string[]

      let hasBranchType = false
      for (const key of keys) {
        const raw = preferencesStore.get(key)
        if (raw) {
          try {
            const config = JSON.parse(raw)
            hasBranchType = (config.template ?? '').includes('{branchType}')
            break
          } catch {
            // try next level
          }
        }
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
    async (_event, payload: { taskKey: string; connectionId?: string; boardId?: string }) => {
      let task: TrackerTask | null = null
      if (payload.taskKey) {
        task = await taskTrackerManager.findTaskByKey(payload.taskKey).catch(() => null)
      }

      let titleTemplate = '[{taskKey}] {taskTitle}'
      let defaultBranch = 'develop'

      const prKeys = [
        payload.boardId &&
          payload.connectionId &&
          `taskTracker.pr.${payload.connectionId}.${payload.boardId}`,
        payload.connectionId && `taskTracker.pr.${payload.connectionId}`,
        'taskTracker.pr',
      ].filter(Boolean) as string[]

      for (const key of prKeys) {
        const raw = preferencesStore.get(key)
        if (raw) {
          try {
            const config = JSON.parse(raw)
            if (config.titleTemplate) titleTemplate = config.titleTemplate
            if (config.defaultBranch) defaultBranch = config.defaultBranch
            break
          } catch {
            // try next level
          }
        }
      }

      if (!prKeys.some((k) => preferencesStore.get(k))) {
        titleTemplate = preferencesStore.get('taskTracker.prTitleTemplate') || titleTemplate
        defaultBranch = preferencesStore.get('taskTracker.prDefaultBranch') || defaultBranch
      }

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

      // Resolve PR config: board → connection → global
      let titleTemplate = '[{taskKey}] {taskTitle}'
      let bodyTemplate = '## {taskKey}: {taskTitle}\n\n{taskUrl}'
      let defaultBranch = 'develop'
      let targetRules: Array<{ taskType: string; targetPattern: string }> = []

      const prKeys = [
        payload.boardId &&
          payload.connectionId &&
          `taskTracker.pr.${payload.connectionId}.${payload.boardId}`,
        payload.connectionId && `taskTracker.pr.${payload.connectionId}`,
        'taskTracker.pr',
      ].filter(Boolean) as string[]

      for (const key of prKeys) {
        const raw = preferencesStore.get(key)
        if (raw) {
          try {
            const config = JSON.parse(raw)
            if (config.titleTemplate) titleTemplate = config.titleTemplate
            if (config.bodyTemplate) bodyTemplate = config.bodyTemplate
            if (config.defaultBranch) defaultBranch = config.defaultBranch
            if (config.targetRules) targetRules = config.targetRules
            break
          } catch {
            // try next level
          }
        }
      }

      // Fallback: read old flat keys if no scoped config found
      if (!prKeys.some((k) => preferencesStore.get(k))) {
        titleTemplate = preferencesStore.get('taskTracker.prTitleTemplate') || titleTemplate
        bodyTemplate = preferencesStore.get('taskTracker.prBodyTemplate') || bodyTemplate
        defaultBranch = preferencesStore.get('taskTracker.prDefaultBranch') || defaultBranch
      }

      const branches = await GitRepository.listBranches(payload.repoRoot)
      const existingBranches = [...branches.local, ...branches.remote]

      const prConfig = buildPRConfig(titleTemplate, bodyTemplate, defaultBranch, targetRules)
      return createPullRequest({
        repoRoot: payload.repoRoot,
        task,
        sourceBranch: payload.sourceBranch,
        prConfig,
        existingBranches,
      })
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

      const worktrees = await GitRepository.listWorktrees(payload.repoRoot)
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
}
