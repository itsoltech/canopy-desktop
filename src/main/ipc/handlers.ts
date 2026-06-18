import { match } from 'ts-pattern'
import {
  ipcMain,
  dialog,
  shell,
  BrowserWindow,
  systemPreferences,
  type IpcMainInvokeEvent,
  type WebContents,
} from 'electron'
import os from 'os'
import fs from 'fs'
import path from 'path'
import { ok, err, type Result } from 'neverthrow'
import type { PtyManager } from '../pty/PtyManager'
import type { TerminalStreamService } from '../pty/TerminalStreamService'
import { TmuxManager as TmuxManagerStatics } from '../pty/TmuxManager'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { LayoutStore } from '../db/LayoutStore'
import type { OnboardingStore } from '../db/OnboardingStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import type { AgentSessionManager } from '../agents/AgentSessionManager'
import type { WindowManager } from '../WindowManager'
import type { BrowserManager } from '../browser/BrowserManager'
import type { CredentialStore, Credential } from '../db/CredentialStore'
import type { TmuxManager } from '../pty/TmuxManager'
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

export interface IpcCommandBridge {
  grantAttachPath(webContentsId: number, targetPath: string): void
}
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
import { fileSystemErrorMessage, type FileSystemError, type FsWriteFileResponse } from './fsErrors'
import { fromExternalCall, errorMessage } from '../errors'

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
import {
  formatTaskContext,
  type TaskAttachmentPath,
  type TaskContextInput,
} from '../taskTracker/taskContext'
import { getBranchTemplate, getPRTemplate } from '../taskTracker/configDefaults'
import type { GitHubService } from '../github/GitHubService'
import { gitHubErrorMessage } from '../github/errors'
import type { RemoteSessionService } from '../remote/RemoteSessionService'
import { remoteServerErrorMessage } from '../remote/errors'
import { listSelectableInterfaces } from '../remote/discovery'
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
import { profileErrorMessage } from '../profiles/errors'
import { KNOWN_AGENT_TYPES, type ProfileInput } from '../profiles/types'
import type { SettingsExportService } from '../settings/SettingsExport'
import { settingsExportErrorMessage } from '../settings/errors'
import type { AgentType } from '../agents/types'
import { WorkspaceCommandService } from '../commands/workspaceCommands'
import { TabCommandService, ToolSessionService } from '../commands/tabCommands'
import { AgentCommandService } from '../commands/agentCommands'
import { RunConfigCommandService } from '../commands/runConfigCommands'
import type { AppStateSnapshot, EditorFileReadResult } from '../commands/types'

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

interface TaskTrackerBranchFromTaskPayload {
  connectionId: string
  task: TrackerTask
  boardId?: string
  branchType?: string
  repoRoot?: string
}

interface TaskTrackerCreateBranchFromTaskPayload extends TaskTrackerBranchFromTaskPayload {
  repoRoot: string
  baseBranch: string
  stashBeforeCreate?: boolean
}

interface TaskTrackerCreateWorktreeFromTaskPayload extends TaskTrackerBranchFromTaskPayload {
  repoRoot: string
  worktreePath: string
  baseBranch: string
}

interface TaskTrackerBuildTaskContextPayload {
  connectionId: string
  task: TaskContextInput
  repoRoot?: string
  trackerId?: string
}

interface WorktreeRemoveWithBranchPayload {
  repoRoot: string
  worktreePath: string
  branch?: string
  deleteBranch?: boolean
  forceOnFailure?: boolean
}

interface WorktreePrepareRemovePayload {
  repoRoot: string
  worktreePath: string
  branch: string
}

interface WorktreePrepareRemoveResult {
  hasUncommittedChanges: boolean
  unmergedCommitCount: number
  branchMerged: boolean
  forceRequired: boolean
  canDeleteBranch: boolean
  warnings: string[]
}

interface WorktreeGetMergedBranchesPayload {
  repoRoot: string
  branches: string[]
}

interface WorktreeGetMergedBranchesResult {
  mergedBranches: string[]
}

interface GitBranchPrepareDeletePayload {
  repoRoot: string
  branch: string
}

interface GitBranchPrepareDeleteResult {
  branchMerged: boolean
  forceRequired: boolean
  warnings: string[]
}

interface GitBranchDeleteWithPreflightPayload {
  repoRoot: string
  branch: string
  forceIfUnmerged?: boolean
}

interface GitBranchDeleteWithPreflightResult {
  branchDeleted: boolean
  forcedBranchDelete: boolean
  branchMerged: boolean
}

type GitPreparePushResult =
  | {
      hasUpstream: false
      confirmationMessage: string
    }
  | {
      hasUpstream: true
      branch: string
      remote: string
      commitCount: number
      confirmationMessage: string
    }

type WorktreeCreatePayload =
  | {
      repoRoot: string
      worktreePath: string
      mode: 'new'
      branch: string
      baseBranch: string
    }
  | {
      repoRoot: string
      worktreePath: string
      mode: 'existing'
      branch: string
      createLocalTracking?: boolean
    }

interface WorktreeCreateResult {
  branch: string
  worktreePath: string
}

interface WorktreeRemoveWithBranchResult {
  worktreeRemoved: boolean
  branchDeleted: boolean
  forcedWorktreeRemove: boolean
  forcedBranchDelete: boolean
}

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

export function registerIpcHandlers(
  ptyManager: PtyManager,
  terminalStreamService: TerminalStreamService,
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
): IpcCommandBridge {
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

  function getAppStateSnapshot(webContentsId: number): AppStateSnapshot {
    return {
      workspace: workspaceCommandService.getSnapshot(webContentsId),
      tabs: tabCommandService.getSnapshot(webContentsId),
    }
  }

  function emitAppStateChanged(sender: WebContents): void {
    if (sender.isDestroyed()) return
    sender.send('app:stateChanged', getAppStateSnapshot(sender.id))
  }

  async function confirmUnsavedChangesDialog(
    sender: WebContents,
    filePaths: string[],
  ): Promise<'save' | 'discard' | 'cancel'> {
    const win = BrowserWindow.fromWebContents(sender) ?? BrowserWindow.getFocusedWindow()
    if (!win) return 'cancel'
    const fileList =
      filePaths.length === 1 ? path.basename(filePaths[0]) : `${filePaths.length} files`
    const { response } = await dialog.showMessageBox(win, {
      type: 'warning',
      buttons: ['Save', "Don't Save", 'Cancel'],
      defaultId: 0,
      cancelId: 2,
      title: 'Unsaved changes',
      message: `You have unsaved changes in ${fileList}`,
      detail: 'Do you want to save them before closing?',
    })
    return match(response)
      .with(0, () => 'save' as const)
      .with(1, () => 'discard' as const)
      .otherwise(() => 'cancel' as const)
  }

  async function writeTextFileWithExpectedMtime(
    webContentsId: number,
    filePath: string,
    content: string,
    expectedMtimeMs?: number,
  ): Promise<FsWriteFileResponse> {
    const resolved = await validatePathAccess(webContentsId, filePath)
    // Structured response instead of a thrown Error — keeps the typed
    // `_tag` discriminant intact across IPC so the renderer branches on
    // `result.tag` instead of string-matching a flattened message.
    if (expectedMtimeMs !== undefined) {
      const statResult = await fromExternalCall(fs.promises.stat(resolved), errorMessage)
      if (statResult.isErr()) return { ok: false, tag: 'StatFailed', message: statResult.error }
      const actualMtimeMs = statResult.value.mtimeMs
      if (Math.abs(actualMtimeMs - expectedMtimeMs) > 1) {
        return { ok: false, tag: 'StaleWrite', actualMtimeMs }
      }
    }
    const writeResult = await fromExternalCall(
      fs.promises.writeFile(resolved, content, 'utf-8'),
      errorMessage,
    )
    if (writeResult.isErr()) {
      return { ok: false, tag: 'WriteFailed', message: writeResult.error }
    }
    const finalStat = await fromExternalCall(fs.promises.stat(resolved), errorMessage)
    if (finalStat.isErr()) return { ok: false, tag: 'StatFailed', message: finalStat.error }
    return { ok: true, mtimeMs: finalStat.value.mtimeMs, size: finalStat.value.size }
  }

  async function loadEditorFileFromDisk(
    webContentsId: number,
    filePath: string,
    maxBytes?: number,
  ): Promise<EditorFileReadResult> {
    let resolved: string
    try {
      resolved = await validatePathAccess(webContentsId, filePath)
    } catch (e) {
      return { ok: false, tag: 'ReadFailed', message: errorMessage(e) }
    }

    const readLimit = Math.min(maxBytes ?? 1_048_576, 10_485_760)
    let stat: fs.Stats
    try {
      stat = await fs.promises.stat(resolved)
    } catch (e) {
      return { ok: false, tag: 'StatFailed', message: errorMessage(e) }
    }

    let canWrite = true
    try {
      await fs.promises.access(resolved, fs.constants.W_OK)
    } catch {
      canWrite = false
    }

    const readSize = Math.min(stat.size, readLimit)
    let fileHandle: Awaited<ReturnType<typeof fs.promises.open>>
    try {
      fileHandle = await fs.promises.open(resolved, 'r')
    } catch (e) {
      return { ok: false, tag: 'ReadFailed', message: errorMessage(e) }
    }

    try {
      const buf = Buffer.alloc(readSize)
      let offset = 0
      while (offset < readSize) {
        const { bytesRead } = await fileHandle.read(buf, offset, readSize - offset, offset)
        if (bytesRead === 0) break
        offset += bytesRead
      }

      const detectEnd = Math.min(offset, 8192)
      for (let i = 0; i < detectEnd; i++) {
        if (buf[i] === 0) {
          return {
            ok: true,
            binary: true,
            size: stat.size,
            canWrite,
            mtimeMs: stat.mtimeMs,
          }
        }
      }

      const content = buf.subarray(0, offset).toString('utf-8')
      return {
        ok: true,
        binary: false,
        content,
        truncated: stat.size > readLimit,
        size: stat.size,
        canWrite,
        mtimeMs: stat.mtimeMs,
        fileLineEnding: content.includes('\r\n') ? 'CRLF' : 'LF',
      }
    } catch (e) {
      return { ok: false, tag: 'ReadFailed', message: errorMessage(e) }
    } finally {
      await fileHandle.close().catch(() => undefined)
    }
  }

  const workspaceCommandService = new WorkspaceCommandService({
    workspaceStore,
    layoutStore,
    windowManager,
    persistWindowConfigs,
    validatePathAccess: (webContentsId, targetPath) =>
      validatePathAccess(webContentsId, targetPath),
    clearWorkspaceFileCache: (workspacePath) => {
      workspaceFileCache.delete(workspacePath)
    },
    emitAppStateChanged,
  })
  const toolSessionService = new ToolSessionService({
    ptyManager,
    terminalStreamService,
    preferencesStore,
    toolRegistry,
    agentSessionManager,
    windowManager,
    tmuxManager,
    profileStore,
    resolveWorkspaceIdForWorktree: (webContentsId, worktreePath) =>
      workspaceCommandService.getWorkspaceIdForWorktree(webContentsId, worktreePath),
  })
  const tabCommandService = new TabCommandService({
    toolSessions: toolSessionService,
    layoutStore,
    browserManager,
    windowManager,
    confirmUnsavedChanges: (sender, filePaths) => confirmUnsavedChangesDialog(sender, filePaths),
    writeEditorFile: (sender, filePath, content, expectedMtimeMs) =>
      writeTextFileWithExpectedMtime(sender.id, filePath, content, expectedMtimeMs),
    loadEditorFile: (sender, filePath, maxBytes) =>
      loadEditorFileFromDisk(sender.id, filePath, maxBytes),
    resolveWorkspaceIdForWorktree: (webContentsId, worktreePath) =>
      workspaceCommandService.getWorkspaceIdForWorktree(webContentsId, worktreePath),
    emitAppStateChanged,
  })
  const agentCommandService = new AgentCommandService({
    ptyManager,
    agentSessionManager,
    windowManager,
  })
  const runConfigCommandService = new RunConfigCommandService({
    ptyManager,
    terminalStreamService,
    windowManager,
    runConfigManager,
    validatePathAccess: (webContentsId, targetPath) =>
      validatePathAccess(webContentsId, targetPath),
  })

  ipcMain.handle(
    'workspace:command:restoreWindow',
    (event, payload: { paths: string[]; activeWorktreePath?: string; removedPaths?: string[] }) =>
      workspaceCommandService.restoreWindow(event.sender, payload),
  )
  ipcMain.handle('workspace:command:attachProject', (event, payload: { path: string }) =>
    workspaceCommandService.attachProject(event.sender, payload.path),
  )
  ipcMain.handle('workspace:command:detachProject', (event, payload: { path: string }) =>
    workspaceCommandService.detachProject(event.sender, payload.path),
  )
  ipcMain.handle('workspace:command:selectWorktree', (event, payload: { path: string }) =>
    workspaceCommandService.selectWorktree(event.sender, payload.path),
  )
  ipcMain.handle('workspace:command:initGitRepo', (event, payload: { path: string }) =>
    workspaceCommandService.initGitRepo(event.sender, payload.path),
  )
  ipcMain.handle('app:getState', (event) => getAppStateSnapshot(event.sender.id))
  ipcMain.handle('app:getStartupRestoreState', (event) => ({
    restoring: windowManager.hasStartupRestore(event.sender.id),
  }))
  ipcMain.handle('app:completeStartupRestore', (event) => {
    windowManager.completeStartupRestore(event.sender.id)
  })
  ipcMain.handle('tab:command:openTool', (event, payload) =>
    tabCommandService.openTool(event.sender, payload),
  )
  ipcMain.handle('tab:command:openDiff', (event, payload) =>
    tabCommandService.openDiffTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:openSessionTab', (event, payload) =>
    tabCommandService.openSessionTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:openEditorFile', (event, payload) =>
    tabCommandService.openEditorFile(event.sender, payload),
  )
  ipcMain.handle('tab:command:detachEditorFile', (event, payload) =>
    tabCommandService.detachEditorFile(event.sender, payload),
  )
  ipcMain.handle('tab:command:closeEditorFile', (event, payload) =>
    tabCommandService.closeEditorFile(event.sender, payload),
  )
  ipcMain.handle('tab:command:prepareCloseEditorFile', (event, payload) =>
    tabCommandService.prepareCloseEditorFile(event.sender, payload),
  )
  ipcMain.handle('tab:command:moveEditorFile', (event, payload) =>
    tabCommandService.moveEditorFile(event.sender, payload),
  )
  ipcMain.handle('tab:command:moveEditorFileBetweenPanes', (event, payload) =>
    tabCommandService.moveEditorFileBetweenPanes(event.sender, payload),
  )
  ipcMain.handle('tab:command:setActiveEditorFile', (event, payload) =>
    tabCommandService.setActiveEditorFile(event.sender, payload),
  )
  ipcMain.handle('tab:command:updateEditorFileState', (event, payload) =>
    tabCommandService.updateEditorFileState(event.sender, payload),
  )
  ipcMain.handle('tab:command:loadEditorFile', (event, payload) =>
    tabCommandService.loadEditorFile(event.sender, payload),
  )
  ipcMain.handle('tab:command:saveEditorFile', (event, payload) =>
    tabCommandService.saveEditorFile(event.sender, payload),
  )
  ipcMain.handle('tab:command:updatePaneTitle', (event, payload) =>
    tabCommandService.updatePaneTitle(event.sender, payload),
  )
  ipcMain.handle('tab:command:updatePaneUrl', (event, payload) =>
    tabCommandService.updatePaneUrl(event.sender, payload),
  )
  ipcMain.handle('tab:command:updateTmuxSessionName', (event, payload) =>
    tabCommandService.updateTmuxSessionName(event.sender, payload),
  )
  ipcMain.handle('tab:command:handlePtyExit', (event, payload) =>
    tabCommandService.handlePtyExit(event.sender, payload),
  )
  ipcMain.handle('tab:command:killTmuxPane', (event, payload) =>
    tabCommandService.killTmuxPane(event.sender, payload),
  )
  ipcMain.handle('tab:command:reattachTmuxPane', (event, payload) =>
    tabCommandService.reattachTmuxPane(event.sender, payload),
  )
  ipcMain.handle('tab:command:toggleFocusedInspector', (event, payload) =>
    tabCommandService.toggleFocusedInspector(event.sender, payload),
  )
  ipcMain.handle('tab:command:restartPane', (event, payload) =>
    tabCommandService.restartPane(event.sender, payload),
  )
  ipcMain.handle('tab:command:closeTab', (event, payload) =>
    tabCommandService.closeTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:prepareCloseTab', (event, payload) =>
    tabCommandService.prepareCloseTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:prepareCloseAllForWorktree', (event, payload) =>
    tabCommandService.prepareCloseAllForWorktree(event.sender, payload),
  )
  ipcMain.handle('tab:command:getCloseWarning', (event, payload) =>
    tabCommandService.getCloseWarning(event.sender, payload),
  )
  ipcMain.handle('tab:command:reopenClosedTab', (event, payload) =>
    tabCommandService.reopenClosedTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:closePane', (event, payload) =>
    tabCommandService.closePane(event.sender, payload),
  )
  ipcMain.handle('tab:command:closeAllForWorktree', (event, payload) =>
    tabCommandService.closeAllForWorktree(event.sender, payload),
  )
  ipcMain.handle('tab:command:setActiveTab', (event, payload) =>
    tabCommandService.setActiveTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:moveTab', (event, payload) =>
    tabCommandService.moveTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:moveTabToSplit', (event, payload) =>
    tabCommandService.moveTabToSplit(event.sender, payload),
  )
  ipcMain.handle('tab:command:movePaneToTarget', (event, payload) =>
    tabCommandService.movePaneToTarget(event.sender, payload),
  )
  ipcMain.handle('tab:command:detachPaneToTab', (event, payload) =>
    tabCommandService.detachPaneToTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:spawnPane', (event, payload) =>
    tabCommandService.spawnPane(event.sender, payload),
  )
  ipcMain.handle('tab:command:splitPane', (event, payload) =>
    tabCommandService.splitPane(event.sender, payload),
  )
  ipcMain.handle('tab:command:focusPane', (event, payload) =>
    tabCommandService.focusPane(event.sender, payload),
  )
  ipcMain.handle('tab:command:navigatePaneFocus', (event, payload) =>
    tabCommandService.navigatePaneFocus(event.sender, payload),
  )
  ipcMain.handle('tab:command:updateSplitRatio', (event, payload) =>
    tabCommandService.updateSplitRatio(event.sender, payload),
  )
  ipcMain.handle('tab:command:restoreLayout', (event, payload) =>
    tabCommandService.restoreLayout(event.sender, payload),
  )
  ipcMain.handle('tab:command:resumeSuspendedTab', (event, payload) =>
    tabCommandService.resumeSuspendedTab(event.sender, payload),
  )
  ipcMain.handle('tab:command:killAll', (event) => tabCommandService.killAll(event.sender))
  ipcMain.handle('tab:command:focusSession', (event, payload) =>
    tabCommandService.focusSession(event.sender, payload),
  )
  ipcMain.handle('tab:command:saveCurrentLayout', (event, payload) =>
    tabCommandService.saveCurrentLayout(event.sender, payload),
  )
  ipcMain.handle('agent:command:sendTaskContext', (event, payload) =>
    agentCommandService.sendTaskContext(event.sender, payload),
  )
  ipcMain.handle('agent:command:sendReviewContext', (event, payload) =>
    agentCommandService.sendReviewContext(event.sender, payload),
  )
  ipcMain.handle('agent:command:sendDrawing', (event, payload) =>
    agentCommandService.sendDrawing(event.sender, payload),
  )
  ipcMain.handle('runConfig:command:execute', (event, payload) =>
    runConfigCommandService.execute(event.sender, payload),
  )
  ipcMain.handle('runConfig:command:listRunning', (event) =>
    runConfigCommandService.listRunning(event.sender),
  )

  // --- PTY ---

  ipcMain.handle(
    'pty:resize',
    (event, payload: { sessionId: string; cols: number; rows: number }) => {
      if (!windowManager.ownsPtySession(event.sender.id, payload.sessionId)) {
        // Terminal resize can arrive late from hidden/unmounted renderer instances
        // or another window that still has stale layout state. Resize is
        // idempotent and carries no input data, so ignore it while keeping
        // mutating PTY operations like write/kill strictly owned.
        return
      }
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

  ipcMain.handle('pty:kill', async (event, payload: { sessionId: string; killTmux?: boolean }) => {
    if (!windowManager.ownsPtySession(event.sender.id, payload.sessionId)) {
      throw new Error('PTY session is not owned by this window')
    }
    await toolSessionService.killPty(payload.sessionId, payload.killTmux)
  })

  ipcMain.handle('pty:write', (event, payload: { sessionId: string; data: string }) => {
    if (!windowManager.ownsPtySession(event.sender.id, payload.sessionId)) {
      throw new Error('PTY session is not owned by this window')
    }
    // Renderer is untrusted: only forward string data into the native PTY.
    if (typeof payload.data !== 'string') {
      throw new Error('pty:write requires string data')
    }
    ptyManager.write(payload.sessionId, payload.data)
  })

  ipcMain.handle('pty:hasChildProcess', (event, payload: { sessionId: string }) => {
    if (!windowManager.ownsPtySession(event.sender.id, payload.sessionId)) {
      throw new Error('PTY session is not owned by this window')
    }
    return ptyManager.hasChildProcess(payload.sessionId)
  })

  ipcMain.handle('pty:getDimensions', (event, payload: { sessionId: string }) => {
    if (!windowManager.ownsPtySession(event.sender.id, payload.sessionId)) {
      throw new Error('PTY session is not owned by this window')
    }
    return ptyManager.getDimensions(payload.sessionId)
  })

  function validatePtyStreamSubscriptionPayload(payload: unknown): {
    sessionId: string
    subscriptionId: string
    offset: number
  } {
    if (!payload || typeof payload !== 'object') {
      throw new Error('pty-stream:subscribe requires an object payload')
    }

    const candidate = payload as {
      sessionId?: unknown
      subscriptionId?: unknown
      offset?: unknown
    }
    if (typeof candidate.sessionId !== 'string' || candidate.sessionId.length === 0) {
      throw new Error('pty-stream:subscribe requires a sessionId')
    }
    if (
      typeof candidate.subscriptionId !== 'string' ||
      candidate.subscriptionId.length === 0 ||
      candidate.subscriptionId.length > 200
    ) {
      throw new Error('pty-stream:subscribe requires a valid subscriptionId')
    }
    if (
      typeof candidate.offset !== 'number' ||
      !Number.isFinite(candidate.offset) ||
      candidate.offset < 0
    ) {
      throw new Error('pty-stream:subscribe requires a non-negative offset')
    }

    return {
      sessionId: candidate.sessionId,
      subscriptionId: candidate.subscriptionId,
      offset: Math.floor(candidate.offset),
    }
  }

  function validatePtyStreamUnsubscribePayload(payload: unknown): { subscriptionId: string } {
    if (!payload || typeof payload !== 'object') {
      throw new Error('pty-stream:unsubscribe requires an object payload')
    }

    const candidate = payload as { subscriptionId?: unknown }
    if (
      typeof candidate.subscriptionId !== 'string' ||
      candidate.subscriptionId.length === 0 ||
      candidate.subscriptionId.length > 200
    ) {
      throw new Error('pty-stream:unsubscribe requires a valid subscriptionId')
    }

    return { subscriptionId: candidate.subscriptionId }
  }

  ipcMain.handle('pty-stream:subscribe', (event, payload: unknown) => {
    const input = validatePtyStreamSubscriptionPayload(payload)
    terminalStreamService.subscribe({
      webContents: event.sender,
      sessionId: input.sessionId,
      subscriptionId: input.subscriptionId,
      offset: input.offset,
    })
  })

  ipcMain.handle('pty-stream:unsubscribe', (event, payload: unknown) => {
    const input = validatePtyStreamUnsubscribePayload(payload)
    return terminalStreamService.unsubscribe(input.subscriptionId, event.sender)
  })

  ipcMain.handle('pty-stream:hasStream', (event, payload: { sessionId: string }) => {
    if (!payload || typeof payload.sessionId !== 'string' || payload.sessionId.length === 0) {
      throw new Error('pty-stream:hasStream requires a sessionId')
    }
    return terminalStreamService.hasStream(event.sender.id, payload.sessionId)
  })

  ipcMain.handle('pty-stream:getDiagnostics', (event) => {
    if (!windowManager.getWindowById(event.sender.id)) {
      throw new Error('PTY stream diagnostics are only available to app windows')
    }
    return terminalStreamService.getDiagnostics()
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
      return toolSessionService.attachTmux(event.sender, payload)
    },
  )

  ipcMain.handle('tmux:detach', (event, payload: { sessionId: string }) => {
    if (!windowManager.ownsPtySession(event.sender.id, payload.sessionId)) {
      throw new Error('PTY session is not owned by this window')
    }
    const tmuxName = ptyManager.getTmuxSessionName(payload.sessionId)
    terminalStreamService.destroy(payload.sessionId)
    ptyManager.kill(payload.sessionId)
    return { tmuxSessionName: tmuxName }
  })

  function senderOwnsTmuxSession(webContentsId: number, name: string): boolean {
    return ptyManager
      .getSessionIdsForTmuxSession(name)
      .some((sessionId) => windowManager.ownsPtySession(webContentsId, sessionId))
  }

  async function assertCanManageTmuxSession(webContentsId: number, name: string): Promise<void> {
    if (senderOwnsTmuxSession(webContentsId, name)) return

    // Detached tmux sessions may survive an app restart without an in-memory
    // PTY owner. Only allow names generated by Canopy on its private socket.
    if (TmuxManagerStatics.isCanopySession(name) && (await tmuxManager.hasSession(name))) {
      return
    }
    throw new Error('tmux session is not managed by Canopy')
  }

  ipcMain.handle('tmux:killSession', async (event, payload: { name: string }) => {
    validateTmuxName(payload.name)
    await assertCanManageTmuxSession(event.sender.id, payload.name)
    await tmuxManager.killSession(payload.name)
  })

  ipcMain.handle(
    'tmux:renameSession',
    async (event, payload: { oldName: string; newName: string }) => {
      validateTmuxName(payload.oldName)
      validateTmuxName(payload.newName)
      await assertCanManageTmuxSession(event.sender.id, payload.oldName)
      await tmuxManager.renameSession(payload.oldName, payload.newName)
      ptyManager.updateTmuxSessionName(payload.oldName, payload.newName)
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

  ipcMain.handle('db:workspace:remove', (_event, payload: { id: string }) => {
    workspaceStore.remove(payload.id)
  })

  // --- Preferences ---

  ipcMain.handle('db:prefs:get', (_event, payload: { key: string }) => {
    // Encrypted keys (API keys, tracker tokens) are main-process-only —
    // returning their plaintext to the renderer would let any compromised
    // page or webview script extract credentials. The renderer reads
    // non-secret prefs only; secrets reach agent processes via env vars
    // built inside the main process.
    if (preferencesStore.isEncrypted(payload.key)) return null
    return preferencesStore.get(payload.key)
  })

  ipcMain.handle('db:prefs:set', async (event, payload: { key: string; value: string }) => {
    // Renderer must not be able to overwrite encrypted pref keys (API keys,
    // tracker tokens) — `preferencesStore.set` auto-encrypts via safeStorage,
    // so a write here would silently replace the user's stored credential
    // with an attacker-controlled value, redirecting agent calls. Secret
    // writes go through `profile:save` / `keychain:setCredentials`.
    if (preferencesStore.isEncrypted(payload.key)) {
      throw new Error(`Refusing to set encrypted preference key "${payload.key}" via db:prefs:set`)
    }
    const previousValue = preferencesStore.get(payload.key)
    preferencesStore.set(payload.key, payload.value)
    const valueChanged = previousValue !== payload.value
    if (payload.key === 'remote.enabled' && valueChanged) {
      if (payload.value === 'false') {
        await remoteSessionService.stop()
      } else {
        await remoteSessionService.ensureListening(event.sender.id).match(
          () => {},
          () => {},
        )
      }
    }
    // Changing the listener bind scope requires rebinding the signaling server.
    // Stop the current session; the next ensureListening() / start() picks
    // up the new pref. Mirrors the enabled=false teardown above.
    const listenerPrefChanged =
      payload.key === 'remote.selectedInterface' || payload.key === 'remote.listenAllInterfaces'
    const listenerSelectionIsIgnored =
      payload.key === 'remote.selectedInterface' &&
      preferencesStore.get('remote.listenAllInterfaces') === 'true'
    if (listenerPrefChanged && valueChanged && !listenerSelectionIsIgnored) {
      const stopped = await remoteSessionService.stop()
      if (stopped.isOk()) {
        await remoteSessionService.ensureListening(event.sender.id).match(
          () => {},
          () => {},
        )
      }
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

  ipcMain.handle('app:showInFolder', async (event, payload: { path: string }) => {
    const resolved = await validatePathAccess(event.sender.id, payload.path)
    shell.showItemInFolder(resolved)
  })

  // --- App: Multi-window ---

  ipcMain.handle('app:newWindow', () => {
    if (windowManager.isQuitting) return
    windowManager.createWindow({
      bounds: cascadeBounds(windowManager.getLastFocusedBounds()),
    })
  })

  ipcMain.handle(
    'app:setFocusedAgentSession',
    (event, payload: { ptySessionId: string | null }) => {
      windowManager.setFocusedAgentSession(event.sender.id, payload.ptySessionId)
    },
  )

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
    if (result.canceled) return null
    const selectedPath = result.filePaths[0]
    if (selectedPath) workspaceCommandService.grantAttachPath(event.sender.id, selectedPath)
    return selectedPath
  })

  ipcMain.handle('dialog:confirmOpenPath', async (event, payload: { path: string }) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (!win || win.isDestroyed()) return null

    let resolved: string
    let home: string
    try {
      resolved = await fs.promises.realpath(path.resolve(payload.path))
      home = await fs.promises.realpath(os.homedir())
    } catch {
      throw new Error('Path does not exist')
    }
    if (resolved !== home && !resolved.startsWith(home + path.sep)) {
      throw new Error('Path must be inside your home directory')
    }

    const { response } = await dialog.showMessageBox(win, {
      type: 'question',
      buttons: ['Open', 'Cancel'],
      defaultId: 1,
      cancelId: 1,
      message: 'Open workspace?',
      detail: `Open this path as a workspace?\n${resolved}`,
    })
    if (response !== 0) return null

    workspaceCommandService.grantAttachPath(event.sender.id, resolved)
    return resolved
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

  ipcMain.handle('git:worktrees', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    return GitRepository.listWorktrees(resolvedRepo).unwrapOr([])
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
    const resolved = await validatePathAccess(event.sender.id, payload.repoRoot)

    const senderId = event.sender.id

    // Only one watcher per window — dispose any previous one first
    windowManager.disposeFileWatcher(senderId)

    const watcher = new FileTreeWatcher(resolved, (events) => {
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

  ipcMain.handle('git:init', async (event, payload: { path: string }) => {
    const resolved = await validatePathAccess(event.sender.id, payload.path)
    await execFileAsync('git', ['init'], { cwd: resolved })
    return GitRepository.detect(resolved).unwrapOr(defaultGitInfo)
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
    async (event, payload: { repoRoot: string; message: string; stageAll?: boolean }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const result = await GitRepository.commit(resolvedRepo, payload.message, payload.stageAll)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:push', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.push(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle(
    'git:commitWorktree',
    async (
      event,
      payload: {
        repoRoot: string
        message: string
        stageAll?: boolean
      },
    ) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const result = await GitRepository.commit(resolvedRepo, payload.message, payload.stageAll)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:pushWorktree', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.push(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:pull', async (event, payload: { repoRoot: string; rebase: boolean }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.pull(resolvedRepo, payload.rebase)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:pullWithPreferences', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const rebase = (preferencesStore.get('gitPullRebase') ?? 'true') !== 'false'
    const result = unwrapOrThrow(await GitRepository.pull(resolvedRepo, rebase), gitErrorMessage)
    return { ...result, rebase }
  })

  ipcMain.handle('git:fetchWorktree', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.fetch(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:fetch', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.fetch(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:fetchAll', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.fetchAll(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:stash', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.stash(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:stashWorktree', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.stash(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:stashPop', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.stashPop(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:stashPopWorktree', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.stashPop(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle('git:branches', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.listBranches(resolvedRepo)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle(
    'git:branchCreate',
    async (event, payload: { repoRoot: string; name: string; baseBranch: string }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const result = await GitRepository.createBranch(
        resolvedRepo,
        payload.name,
        payload.baseBranch,
      )
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'git:branchCreateFromHead',
    async (event, payload: { repoRoot: string; branch: string }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const result = await GitRepository.createBranch(resolvedRepo, payload.branch, 'HEAD')
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:checkout', async (event, payload: { repoRoot: string; branch: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.checkout(resolvedRepo, payload.branch)
    return unwrapOrThrow(result, gitErrorMessage)
  })

  ipcMain.handle(
    'git:branchDelete',
    async (event, payload: { repoRoot: string; name: string; force: boolean }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const result = await GitRepository.deleteBranch(resolvedRepo, payload.name, payload.force)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'git:branchPrepareDelete',
    async (
      event,
      payload: GitBranchPrepareDeletePayload,
    ): Promise<GitBranchPrepareDeleteResult> => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const branchMerged = unwrapOrThrow(
        await GitRepository.isBranchMerged(resolvedRepo, payload.branch),
        gitErrorMessage,
      )
      const warnings = branchMerged ? [] : ['Branch has not been fully merged.']

      return {
        branchMerged,
        forceRequired: !branchMerged,
        warnings,
      }
    },
  )

  ipcMain.handle(
    'git:branchDeleteWithPreflight',
    async (
      event,
      payload: GitBranchDeleteWithPreflightPayload,
    ): Promise<GitBranchDeleteWithPreflightResult> => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const branchMerged = unwrapOrThrow(
        await GitRepository.isBranchMerged(resolvedRepo, payload.branch),
        gitErrorMessage,
      )
      const forceDelete = !branchMerged && Boolean(payload.forceIfUnmerged)

      if (!branchMerged && !forceDelete) {
        throw new Error('Branch has not been fully merged.')
      }

      const result = await GitRepository.deleteBranch(resolvedRepo, payload.branch, forceDelete)
      unwrapOrThrow(result, gitErrorMessage)

      return {
        branchDeleted: true,
        forcedBranchDelete: forceDelete,
        branchMerged,
      }
    },
  )

  ipcMain.handle(
    'git:branchDeleteRemote',
    async (event, payload: { repoRoot: string; remote: string; name: string }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const result = await GitRepository.deleteRemoteBranch(
        resolvedRepo,
        payload.remote,
        payload.name,
      )
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:pushInfo', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    return GitRepository.getPushInfo(resolvedRepo).unwrapOr(null)
  })

  ipcMain.handle(
    'git:preparePush',
    async (event, payload: { repoRoot: string }): Promise<GitPreparePushResult> => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const info = await GitRepository.getPushInfo(resolvedRepo).unwrapOr(null)
      if (!info) {
        return {
          hasUpstream: false,
          confirmationMessage: 'No upstream branch — push and set tracking to origin?',
        }
      }

      return {
        hasUpstream: true,
        ...info,
        confirmationMessage: `Push ${info.commitCount} commit(s) to ${info.remote}/${info.branch}?`,
      }
    },
  )

  ipcMain.handle(
    'git:branchMerged',
    async (event, payload: { repoRoot: string; branch: string }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      return GitRepository.isBranchMerged(resolvedRepo, payload.branch).unwrapOr(false)
    },
  )

  ipcMain.handle(
    'git:worktreeAdd',
    async (
      event,
      payload: { repoRoot: string; path: string; branch: string; baseBranch: string },
    ) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const expanded = payload.path.startsWith('~/')
        ? os.homedir() + payload.path.slice(1)
        : payload.path
      const resolvedPath = await validateWorktreeCreationPath(event.sender.id, expanded)
      const result = await GitRepository.worktreeAdd(
        resolvedRepo,
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
      event,
      payload: {
        repoRoot: string
        path: string
        branch: string
        createLocalTracking: boolean
      },
    ) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const expanded = payload.path.startsWith('~/')
        ? os.homedir() + payload.path.slice(1)
        : payload.path
      const resolvedPath = await validateWorktreeCreationPath(event.sender.id, expanded)
      const result = await GitRepository.worktreeAddCheckout(
        resolvedRepo,
        resolvedPath,
        payload.branch,
        payload.createLocalTracking,
      )
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'worktree:create',
    async (event, payload: WorktreeCreatePayload): Promise<WorktreeCreateResult> => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const expandedPath = payload.worktreePath.startsWith('~/')
        ? os.homedir() + payload.worktreePath.slice(1)
        : payload.worktreePath
      const worktreePath = await validateWorktreeCreationPath(event.sender.id, expandedPath)

      if (payload.mode === 'new') {
        const result = await GitRepository.worktreeAdd(
          resolvedRepo,
          worktreePath,
          payload.branch,
          payload.baseBranch,
        )
        unwrapOrThrow(result, gitErrorMessage)
      } else {
        const result = await GitRepository.worktreeAddCheckout(
          resolvedRepo,
          worktreePath,
          payload.branch,
          payload.createLocalTracking ?? false,
        )
        unwrapOrThrow(result, gitErrorMessage)
      }

      return { branch: payload.branch, worktreePath }
    },
  )

  ipcMain.handle(
    'git:worktreeRemove',
    async (event, payload: { repoRoot: string; path: string; force: boolean }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const resolvedTarget = await validateWorktreeExistingPath(event.sender.id, payload.path)
      const result = await GitRepository.worktreeRemove(resolvedRepo, resolvedTarget, payload.force)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'worktree:prepareRemove',
    async (event, payload: WorktreePrepareRemovePayload): Promise<WorktreePrepareRemoveResult> => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const resolvedTarget = await validateWorktreeExistingPath(
        event.sender.id,
        payload.worktreePath,
      )
      const isDetached = payload.branch === '(detached)'

      const statusResult = await GitRepository.getStatusPorcelain(resolvedRepo, resolvedTarget)
      const status = unwrapOrThrow(statusResult, gitErrorMessage)
      const hasUncommittedChanges = status.trim().length > 0

      const unmergedCommits = isDetached
        ? []
        : unwrapOrThrow(
            await GitRepository.getUnmergedCommits(resolvedRepo, payload.branch),
            gitErrorMessage,
          )
      const branchMerged = isDetached
        ? false
        : unwrapOrThrow(
            await GitRepository.isBranchMerged(resolvedRepo, payload.branch),
            gitErrorMessage,
          )

      const warnings: string[] = []
      if (hasUncommittedChanges) warnings.push('Has uncommitted changes.')
      if (unmergedCommits.length > 0) {
        warnings.push(`${unmergedCommits.length} unmerged commit(s) not on any remote.`)
      }

      return {
        hasUncommittedChanges,
        unmergedCommitCount: unmergedCommits.length,
        branchMerged,
        forceRequired: warnings.length > 0,
        canDeleteBranch: !isDetached && branchMerged,
        warnings,
      }
    },
  )

  ipcMain.handle(
    'worktree:getMergedBranches',
    async (
      event,
      payload: WorktreeGetMergedBranchesPayload,
    ): Promise<WorktreeGetMergedBranchesResult> => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const requestedBranches = Array.from(
        new Set(payload.branches.filter((branch) => branch && branch !== '(detached)')),
      )
      if (requestedBranches.length === 0) return { mergedBranches: [] }

      const mergedBranches = unwrapOrThrow(
        await GitRepository.getMergedBranches(resolvedRepo),
        gitErrorMessage,
      )
      const mergedSet = new Set(mergedBranches)

      return {
        mergedBranches: requestedBranches.filter((branch) => mergedSet.has(branch)),
      }
    },
  )

  ipcMain.handle('worktree:listBranches', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    return unwrapOrThrow(await GitRepository.listBranches(resolvedRepo), gitErrorMessage)
  })

  ipcMain.handle('worktree:refreshBranches', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    unwrapOrThrow(await GitRepository.fetchAll(resolvedRepo), gitErrorMessage)
    return unwrapOrThrow(await GitRepository.listBranches(resolvedRepo), gitErrorMessage)
  })

  ipcMain.handle(
    'worktree:removeWithBranch',
    async (
      event,
      payload: WorktreeRemoveWithBranchPayload,
    ): Promise<WorktreeRemoveWithBranchResult> => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const resolvedTarget = await validateWorktreeExistingPath(
        event.sender.id,
        payload.worktreePath,
      )
      const forceOnFailure = payload.forceOnFailure ?? false
      const shouldDeleteBranch =
        payload.deleteBranch ?? Boolean(payload.branch && payload.branch !== '(detached)')

      let forcedWorktreeRemove = false
      try {
        const result = await GitRepository.worktreeRemove(resolvedRepo, resolvedTarget, false)
        unwrapOrThrow(result, gitErrorMessage)
      } catch (e) {
        if (!forceOnFailure) throw e
        const forcedResult = await GitRepository.worktreeRemove(resolvedRepo, resolvedTarget, true)
        unwrapOrThrow(forcedResult, gitErrorMessage)
        forcedWorktreeRemove = true
      }

      let branchDeleted = false
      let forcedBranchDelete = false
      if (shouldDeleteBranch) {
        if (!payload.branch || payload.branch === '(detached)') {
          throw new Error('Branch name is required when deleteBranch is true')
        }
        try {
          const result = await GitRepository.deleteBranch(resolvedRepo, payload.branch, false)
          unwrapOrThrow(result, gitErrorMessage)
          branchDeleted = true
        } catch (e) {
          if (!forceOnFailure) throw e
          const forcedResult = await GitRepository.deleteBranch(resolvedRepo, payload.branch, true)
          unwrapOrThrow(forcedResult, gitErrorMessage)
          branchDeleted = true
          forcedBranchDelete = true
        }
      }

      return {
        worktreeRemoved: true,
        branchDeleted,
        forcedWorktreeRemove,
        forcedBranchDelete,
      }
    },
  )

  ipcMain.handle(
    'git:unmergedCommits',
    async (event, payload: { repoRoot: string; branch: string }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      return GitRepository.getUnmergedCommits(resolvedRepo, payload.branch).unwrapOr([])
    },
  )

  ipcMain.handle(
    'git:statusPorcelain',
    async (event, payload: { repoRoot: string; worktreePath?: string }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      return GitRepository.getStatusPorcelain(resolvedRepo, payload.worktreePath).unwrapOr('')
    },
  )

  ipcMain.handle('git:diff', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const result = await GitRepository.getDiffParsed(resolvedRepo)
    return result.unwrapOr({ files: [] })
  })

  function validateFilePath(filePath: string): void {
    if (filePath.startsWith('-')) throw new Error('Invalid file path: must not start with -')
    if (filePath.startsWith('/')) throw new Error('Invalid file path: must be relative')
    if (filePath.includes('..')) throw new Error('Invalid file path: must not contain ..')
  }

  ipcMain.handle('changes:getDiff', async (event, payload: { worktreePath: string }) => {
    const resolvedWorktree = await validateWorktreeScopedPathAccess(
      event.sender.id,
      payload.worktreePath,
    )
    const result = await GitRepository.getDiffParsed(resolvedWorktree)
    return result.unwrapOr({ files: [] })
  })

  ipcMain.handle(
    'changes:stageFile',
    async (event, payload: { worktreePath: string; filePath: string }) => {
      const resolvedWorktree = await validateWorktreeScopedPathAccess(
        event.sender.id,
        payload.worktreePath,
      )
      validateFilePath(payload.filePath)
      const result = await GitRepository.stageFile(resolvedWorktree, payload.filePath)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'changes:revertFile',
    async (event, payload: { worktreePath: string; filePath: string }) => {
      const resolvedWorktree = await validateWorktreeScopedPathAccess(
        event.sender.id,
        payload.worktreePath,
      )
      validateFilePath(payload.filePath)
      const result = await GitRepository.revertFile(resolvedWorktree, payload.filePath)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:diffFile', async (event, payload: { repoRoot: string; filePath: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    validateFilePath(payload.filePath)
    const result = await GitRepository.getFileDiff(resolvedRepo, payload.filePath)
    return result.unwrapOr({ files: [] })
  })

  ipcMain.handle(
    'git:stageFile',
    async (event, payload: { repoRoot: string; filePath: string }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      validateFilePath(payload.filePath)
      const result = await GitRepository.stageFile(resolvedRepo, payload.filePath)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle(
    'git:revertFile',
    async (event, payload: { repoRoot: string; filePath: string }) => {
      const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      validateFilePath(payload.filePath)
      const result = await GitRepository.revertFile(resolvedRepo, payload.filePath)
      return unwrapOrThrow(result, gitErrorMessage)
    },
  )

  ipcMain.handle('git:generateCommitMessage', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
    const diff = await GitRepository.getDiff(resolvedRepo).unwrapOr('')
    if (!diff.trim()) return null
    return generateCommitMessage(diff, preferencesStore)
  })

  ipcMain.handle(
    'git:createPR',
    async (
      event,
      payload: {
        repoRoot: string
        title: string
        body: string
        baseRefName: string
        draft: boolean
      },
    ) => {
      const resolvedRepo = await validatePathAccess(event.sender.id, payload.repoRoot)
      if (typeof payload.baseRefName !== 'string' || payload.baseRefName.startsWith('-')) {
        throw new Error('Invalid baseRefName')
      }
      const pushResult = await GitRepository.push(resolvedRepo)
      if (pushResult.isErr()) {
        throw new Error(`Failed to push branch: ${gitErrorMessage(pushResult.error)}`)
      }

      const branch = await GitRepository.getBranch(resolvedRepo).unwrapOr(null)
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
        const { stdout } = await execFileAsync('gh', args, { cwd: resolvedRepo })
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

  ipcMain.handle('git:getDefaultBranch', async (event, payload: { repoRoot: string }) => {
    let resolvedRepo: string
    try {
      resolvedRepo = await validatePathAccess(event.sender.id, payload.repoRoot)
    } catch {
      return 'main'
    }
    try {
      const { stdout } = await execFileAsync(
        'gh',
        ['repo', 'view', '--json', 'defaultBranchRef', '--jq', '.defaultBranchRef.name'],
        { cwd: resolvedRepo },
      )
      return stdout.trim() || 'main'
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
        console.warn('[git:getDefaultBranch] gh CLI not found, falling back to "main"')
      }
      return 'main'
    }
  })

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
    // Validate the shape before touching it: Buffer.from(number) would allocate
    // that many bytes (a ~1 GB DoS) and Buffer.from(string) would silently
    // encode attacker text, both before the size cap below can run.
    if (!(payload?.buffer instanceof Uint8Array)) {
      throw new Error('Capture buffer must be binary data')
    }
    // Cap renderer-supplied buffers so a hostile webview-driven save can't
    // exhaust /tmp. 25 MB comfortably covers a full-page PNG of any sane
    // viewport while bounding worst-case disk pressure.
    const MAX_CAPTURE_BYTES = 25 * 1024 * 1024
    if (payload.buffer.length > MAX_CAPTURE_BYTES) {
      throw new Error(`Capture buffer exceeds ${MAX_CAPTURE_BYTES} bytes`)
    }
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

  function comparablePath(value: string): string {
    return process.platform === 'win32' ? value.toLowerCase() : value
  }

  function samePath(a: string, b: string): boolean {
    return comparablePath(a) === comparablePath(b)
  }

  function sameOrChildPath(targetPath: string, basePath: string): boolean {
    const target = comparablePath(targetPath)
    const base = comparablePath(basePath)
    return target === base || target.startsWith(base + path.sep)
  }

  async function normalizeRealpathOrInput(targetPath: string): Promise<string> {
    const result = await fromExternalCall(fs.promises.realpath(targetPath), () => null)
    return path.normalize(result.isOk() ? result.value : targetPath)
  }

  // Realpath-normalize workspace roots too: otherwise a macOS workspace at
  // `/var/...` (which resolves to `/private/var/...`) would never match a
  // target's realpath and all reads get rejected; conversely a symlinked
  // workspace root could fail to block a sibling path under the same
  // unnormalized prefix. Returns the resolved target path so callers can
  // open/stat/readdir it directly, closing the TOCTOU window between
  // validation and the filesystem call.
  async function validatePathAccess(wcId: number, targetPath: string): Promise<string> {
    const resolved = path.normalize(await fs.promises.realpath(targetPath))
    const allowed = windowManager.getWorkspacePaths(wcId)
    const resolvedAllowed = await Promise.all(allowed.map(normalizeRealpathOrInput))
    const ok = resolvedAllowed.some((normalWp) => sameOrChildPath(resolved, normalWp))
    if (!ok) throw new Error('Access denied: path outside workspace')
    return resolved
  }

  async function validateWorktreeScopedPathAccess(
    wcId: number,
    targetPath: string,
  ): Promise<string> {
    const strictAccess = await fromExternalCall(validatePathAccess(wcId, targetPath), (e) => e)
    if (strictAccess.isOk()) return strictAccess.value

    const accessError = strictAccess.error
    const targetRealpath = await fromExternalCall(
      fs.promises.realpath(targetPath),
      () => accessError,
    )
    if (targetRealpath.isErr()) throw targetRealpath.error

    const resolved = path.normalize(targetRealpath.value)
    const ownedPaths = workspaceCommandService
      .getSnapshot(wcId)
      .projects.flatMap((project) => [
        project.workspace.path,
        ...(project.repoRoot ? [project.repoRoot] : []),
        ...project.worktrees.map((worktree) => worktree.path),
      ])
    const resolvedOwnedPaths = await Promise.all(ownedPaths.map(normalizeRealpathOrInput))
    const ok = resolvedOwnedPaths.some((ownedPath) => samePath(resolved, ownedPath))
    if (!ok) throw accessError
    return resolved
  }

  async function readFileTreeDir(
    webContentsId: number,
    dirPath: string,
  ): Promise<Array<{ name: string; isDirectory: boolean; size: number }>> {
    const resolved = await validatePathAccess(webContentsId, dirPath)
    const entries = await fs.promises.readdir(resolved, { withFileTypes: true })
    const ignorePatterns = getIgnorePatterns()
    const filtered = entries.filter((e) => !isIgnoredEntry(e.name, ignorePatterns))
    const results = await Promise.all(
      filtered.map(async (entry) => {
        const isDir = entry.isDirectory()
        let size = 0
        if (!isDir) {
          try {
            const s = await fs.promises.stat(path.join(resolved, entry.name))
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
  }

  ipcMain.handle('fileTree:readDir', async (event, payload: { dirPath: string }) => {
    return readFileTreeDir(event.sender.id, payload.dirPath)
  })

  function parseFileTreeGitStatus(porcelain: string): {
    statuses: Record<string, string>
    affectedPaths: string[]
    changedDirs: string[]
  } {
    const statuses: Record<string, string> = {}
    const affectedPaths = new Set<string>()
    const changedDirs = new Set<string>()

    const collectPath = (rawPath: string): void => {
      const normalized = rawPath.trim()
      if (!normalized) return
      affectedPaths.add(normalized)

      const segments = normalized.split('/')
      let dir = ''
      for (let i = 0; i < segments.length - 1; i++) {
        dir = dir === '' ? segments[i] : `${dir}/${segments[i]}`
        changedDirs.add(dir)
      }
    }

    for (const line of porcelain.split('\n')) {
      if (line.length < 4) continue
      const xy = line.substring(0, 2)
      const filePath = line.substring(3)
      const status = xy[0] !== ' ' && xy[0] !== '?' ? xy[0] : xy[1]
      statuses[filePath] = status === '?' ? '?' : status
      for (const part of filePath.split(' -> ')) collectPath(part)
    }

    return {
      statuses,
      affectedPaths: [...affectedPaths].sort(),
      changedDirs: [...changedDirs].sort(),
    }
  }

  ipcMain.handle(
    'fileTree:getGitStatus',
    async (
      event,
      payload: { repoRoot: string; worktreePath: string },
    ): Promise<{
      statuses: Record<string, string>
      affectedPaths: string[]
      changedDirs: string[]
    }> => {
      const repoRoot = await validatePathAccess(event.sender.id, payload.repoRoot)
      const worktreePath = await validatePathAccess(event.sender.id, payload.worktreePath)
      const porcelain = await GitRepository.getStatusPorcelain(repoRoot, worktreePath).unwrapOr('')
      return parseFileTreeGitStatus(porcelain)
    },
  )

  ipcMain.handle('fs:readDir', async (event, payload: { dirPath: string }) => {
    return readFileTreeDir(event.sender.id, payload.dirPath)
  })

  ipcMain.handle('fs:readFile', async (event, payload: { filePath: string; maxBytes?: number }) => {
    const resolved = await validatePathAccess(event.sender.id, payload.filePath)
    const maxBytes = Math.min(payload.maxBytes ?? 1_048_576, 10_485_760)
    // Sync stat too: closes the TOCTOU gap with the openSync below and removes
    // the last await between validatePathAccess and the fd trio. Operating on
    // the realpath'd target prevents a symlink swap between validation and
    // open from redirecting the read outside the workspace.
    const size = fs.statSync(resolved).size
    const readSize = Math.min(size, maxBytes)

    // Sync fd trio instead of async FileHandle: avoids holding a JS FileHandle
    // across multiple `await` points, which is the only call site in this
    // codebase that exposes us to FileHandle::CloseReq::Resolve races (#150).
    // A bounded read (≤10 MB) from local disk is fast enough to run inline.
    const fd = fs.openSync(resolved, 'r')
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

  // Quick Open — LRU-bounded workspace file listing. Uses `git ls-files` when
  // the worktree is a git repo (respects `.gitignore` automatically); falls
  // back to a recursive readdir filtered by the same ignore patterns as
  // `fs:readDir` so secrets (.env, *.pem, credentials) never land in the
  // picker for non-git workspaces.
  const WORKSPACE_FILE_CACHE_MAX_AGE_MS = 60_000
  const WORKSPACE_FILE_CACHE_MAX_ENTRIES = 16
  // Map iteration order is insertion order, so re-inserting on read turns it
  // into a plain LRU without pulling in a dedicated library.
  const workspaceFileCache = new Map<string, { files: string[]; fetchedAt: number }>()

  function rememberWorkspaceFiles(key: string, files: string[]): void {
    workspaceFileCache.delete(key)
    workspaceFileCache.set(key, { files, fetchedAt: Date.now() })
    while (workspaceFileCache.size > WORKSPACE_FILE_CACHE_MAX_ENTRIES) {
      const oldest = workspaceFileCache.keys().next().value
      if (oldest === undefined) break
      workspaceFileCache.delete(oldest)
    }
  }

  function touchWorkspaceFiles(key: string): { files: string[]; fetchedAt: number } | null {
    const entry = workspaceFileCache.get(key)
    if (!entry) return null
    // Refresh LRU position on read.
    workspaceFileCache.delete(key)
    workspaceFileCache.set(key, entry)
    return entry
  }

  async function listWorkspaceFilesViaGit(repoRoot: string): Promise<string[] | null> {
    try {
      const { stdout } = await execFileAsync(
        'git',
        ['-C', repoRoot, 'ls-files', '--cached', '--others', '--exclude-standard'],
        { maxBuffer: 64 * 1024 * 1024 },
      )
      return stdout.split('\n').filter((l) => l.length > 0)
    } catch {
      return null
    }
  }

  async function listWorkspaceFilesViaReaddir(root: string): Promise<string[]> {
    const ignorePatterns = getIgnorePatterns()
    const files: string[] = []
    async function walk(dir: string, relPrefix: string): Promise<void> {
      let entries: fs.Dirent[]
      try {
        entries = await fs.promises.readdir(dir, { withFileTypes: true })
      } catch {
        return
      }
      for (const entry of entries) {
        if (isIgnoredEntry(entry.name, ignorePatterns)) continue
        const rel = relPrefix ? `${relPrefix}/${entry.name}` : entry.name
        if (entry.isDirectory()) {
          await walk(path.join(dir, entry.name), rel)
        } else if (entry.isFile()) {
          files.push(rel)
        }
      }
    }
    await walk(root, '')
    return files
  }

  ipcMain.handle(
    'quickOpen:listFiles',
    async (event, payload: { worktreePath: string; force?: boolean }): Promise<string[]> => {
      const resolved = await validatePathAccess(event.sender.id, payload.worktreePath)
      if (!payload.force) {
        const cached = touchWorkspaceFiles(resolved)
        if (cached && Date.now() - cached.fetchedAt < WORKSPACE_FILE_CACHE_MAX_AGE_MS) {
          return cached.files
        }
      }
      const viaGit = await listWorkspaceFilesViaGit(resolved)
      const files = viaGit ?? (await listWorkspaceFilesViaReaddir(resolved))
      rememberWorkspaceFiles(resolved, files)
      return files
    },
  )

  ipcMain.handle(
    'quickOpen:invalidateCache',
    async (event, payload: { worktreePath: string }): Promise<void> => {
      const resolved = await validatePathAccess(event.sender.id, payload.worktreePath)
      workspaceFileCache.delete(resolved)
    },
  )

  ipcMain.handle(
    'fs:writeFile',
    async (
      event,
      payload: { filePath: string; content: string; expectedMtimeMs?: number },
    ): Promise<FsWriteFileResponse> => {
      return writeTextFileWithExpectedMtime(
        event.sender.id,
        payload.filePath,
        payload.content,
        payload.expectedMtimeMs,
      )
    },
  )

  // Validate a path that does not yet exist (for create operations). Walks up
  // to the closest existing ancestor, validates IT through validatePathAccess
  // (so realpath/symlink/workspace checks still apply), then re-attaches the
  // not-yet-existing tail. Rejects any path whose tail would escape the
  // ancestor via `..` or absolute references.
  async function validateCreationPath(wcId: number, targetPath: string): Promise<string> {
    const absolute = path.isAbsolute(targetPath) ? targetPath : path.resolve(targetPath)
    const normalized = path.normalize(absolute)
    let ancestor = normalized
    const tail: string[] = []
    while (true) {
      try {
        await fs.promises.access(ancestor)
        break
      } catch {
        const parent = path.dirname(ancestor)
        if (parent === ancestor) throw new Error('Access denied: no existing ancestor')
        tail.unshift(path.basename(ancestor))
        ancestor = parent
      }
    }
    const resolvedAncestor = await validatePathAccess(wcId, ancestor)
    const finalPath = path.join(resolvedAncestor, ...tail)
    const rel = path.relative(resolvedAncestor, finalPath)
    if (rel.startsWith('..') || path.isAbsolute(rel)) {
      throw new Error('Access denied: path escapes workspace')
    }
    return finalPath
  }

  // Containment check shared by worktree validators: a resolved (realpath'd)
  // path is acceptable if it lives under the user's home directory OR any
  // workspace path registered to this window. Windows paths compare
  // case-insensitively to match validatePathAccess.
  async function isUnderHomeOrWorkspace(wcId: number, resolved: string): Promise<boolean> {
    const homeReal = await normalizeRealpathOrInput(os.homedir())
    const allowed = windowManager.getWorkspacePaths(wcId)
    const resolvedAllowed = await Promise.all(allowed.map(normalizeRealpathOrInput))
    const bases = [homeReal, ...resolvedAllowed]
    return bases.some((base) => sameOrChildPath(resolved, base))
  }

  // Worktrees are intentionally created OUTSIDE the workspace by design (the
  // default `worktrees.baseDir` pref is `~/canopy/worktrees`), so the strict
  // workspace-only containment used by validateCreationPath wrongly blocks
  // them — most visibly on Windows where the home dir is never a workspace
  // ancestor. This validator applies the same TOCTOU-safe ancestor walk and
  // escape-via-`..` rejection, but relaxes containment to "under home OR
  // workspace", which still prevents writes to system locations like /etc or
  // C:\Program Files while permitting the documented worktree layout.
  async function validateWorktreeCreationPath(wcId: number, targetPath: string): Promise<string> {
    if (!path.isAbsolute(targetPath)) {
      throw new Error('Worktree path must be absolute')
    }
    const normalized = path.normalize(targetPath)
    let ancestor = normalized
    const tail: string[] = []
    while (true) {
      try {
        await fs.promises.access(ancestor)
        break
      } catch {
        const parent = path.dirname(ancestor)
        if (parent === ancestor) throw new Error('Access denied: no existing ancestor')
        tail.unshift(path.basename(ancestor))
        ancestor = parent
      }
    }
    const resolvedAncestor = path.normalize(await fs.promises.realpath(ancestor))
    if (!(await isUnderHomeOrWorkspace(wcId, resolvedAncestor))) {
      throw new Error('Access denied: worktree path outside home or workspace')
    }
    const finalPath = path.join(resolvedAncestor, ...tail)
    const rel = path.relative(resolvedAncestor, finalPath)
    if (rel.startsWith('..') || path.isAbsolute(rel)) {
      throw new Error('Access denied: worktree path escapes ancestor')
    }
    return finalPath
  }

  // Same relaxed containment for existing worktree paths (remove). Removing
  // an inactive worktree under ~/canopy/worktrees would otherwise fail
  // because only the *active* worktree is in WindowManager's allow-list.
  async function validateWorktreeExistingPath(wcId: number, targetPath: string): Promise<string> {
    if (!path.isAbsolute(targetPath)) {
      throw new Error('Worktree path must be absolute')
    }
    const resolved = path.normalize(await fs.promises.realpath(targetPath))
    if (!(await isUnderHomeOrWorkspace(wcId, resolved))) {
      throw new Error('Access denied: worktree path outside home or workspace')
    }
    return resolved
  }

  async function createFileTreeFile(webContentsId: number, filePath: string): Promise<void> {
    const resolved = await validateCreationPath(webContentsId, filePath)
    // Ensure the parent dir exists (handles nested paths typed in the prompt
    // like "subdir/newfile.ts" — creates "subdir" if missing).
    await fs.promises.mkdir(path.dirname(resolved), { recursive: true })
    // wx flag: error if file already exists. Avoids silently clobbering
    // anything the user typed by accident.
    const handle = await fs.promises.open(resolved, 'wx')
    await handle.close()
  }

  async function createFileTreeDirectory(webContentsId: number, dirPath: string): Promise<void> {
    const resolved = await validateCreationPath(webContentsId, dirPath)
    await fs.promises.mkdir(resolved, { recursive: true })
  }

  ipcMain.handle(
    'fileTree:createFile',
    async (event, payload: { filePath: string }): Promise<void> => {
      await createFileTreeFile(event.sender.id, payload.filePath)
    },
  )

  ipcMain.handle(
    'fileTree:createDirectory',
    async (event, payload: { dirPath: string }): Promise<void> => {
      await createFileTreeDirectory(event.sender.id, payload.dirPath)
    },
  )

  ipcMain.handle('fs:createFile', async (event, payload: { filePath: string }): Promise<void> => {
    await createFileTreeFile(event.sender.id, payload.filePath)
  })

  ipcMain.handle('fs:mkdir', async (event, payload: { dirPath: string }): Promise<void> => {
    await createFileTreeDirectory(event.sender.id, payload.dirPath)
  })

  ipcMain.handle(
    'dialog:confirmUnsavedChanges',
    async (event, payload: { filePaths: string[] }): Promise<'save' | 'discard' | 'cancel'> => {
      return confirmUnsavedChangesDialog(event.sender, payload.filePaths)
    },
  )

  ipcMain.handle(
    'fs:stat',
    async (
      event,
      payload: { filePath: string },
    ): Promise<{ mtimeMs: number; size: number; canWrite: boolean }> => {
      const resolved = await validatePathAccess(event.sender.id, payload.filePath)
      // Async fs — sync calls here stalled the main process on every editor
      // load (stat) and every write-permission check (access).
      const statResult = await fromExternalCall(
        fs.promises.stat(resolved),
        (e): FileSystemError => ({ _tag: 'StatFailed', message: errorMessage(e) }),
      )
      if (statResult.isErr()) {
        throw new Error(fileSystemErrorMessage(statResult.error))
      }
      const canWrite = await fs.promises
        .access(resolved, fs.constants.W_OK)
        .then(() => true)
        .catch(() => false)
      return { mtimeMs: statResult.value.mtimeMs, size: statResult.value.size, canWrite }
    },
  )

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

  ipcMain.handle('repoConfig:load', async (event, payload: { repoRoot: string }) => {
    const resolved = await validatePathAccess(event.sender.id, payload.repoRoot)
    const result = await repoConfigManager.load(resolved)
    return result.unwrapOr(null)
  })

  ipcMain.handle(
    'repoConfig:save',
    async (event, payload: { repoRoot: string; config: unknown }) => {
      const resolved = await validatePathAccess(event.sender.id, payload.repoRoot)
      if (!isValidRepoConfig(payload.config)) {
        throw new Error('Invalid config: check version, trackers, filters, and template fields')
      }
      const result = await repoConfigManager.save(resolved, payload.config)
      unwrapOrThrow(result, taskTrackerErrorMessage)
    },
  )

  ipcMain.handle('repoConfig:exists', async (event, payload: { repoRoot: string }) => {
    const resolved = await validatePathAccess(event.sender.id, payload.repoRoot)
    return repoConfigManager.exists(resolved)
  })

  ipcMain.handle('repoConfig:init', async (event, payload: { repoRoot: string }) => {
    const resolved = await validatePathAccess(event.sender.id, payload.repoRoot)
    const result = await repoConfigManager.init(resolved)
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

  async function resolveTaskTrackerBranchName(
    payload: TaskTrackerBranchFromTaskPayload,
  ): Promise<string> {
    const resolved = await resolveEffectiveConfig(payload.repoRoot)
    const branchTpl = resolved
      ? getBranchTemplate(resolved.config, payload.boardId)
      : { template: '{taskKey}', customVars: {} }

    const sprint = resolved
      ? await taskTrackerManager
          .getCurrentSprintFromConfig(resolved.config, payload.boardId, payload.repoRoot)
          .unwrapOr(null)
      : await taskTrackerManager
          .getCurrentSprint(payload.connectionId, payload.boardId)
          .unwrapOr(null)

    const variables = buildVariables(payload.task, sprint, branchTpl.customVars, payload.branchType)
    return renderBranchName(branchTpl.template, variables)
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
      if (!resolved) return []
      const result = await taskTrackerManager.fetchBoardsFromConfig(
        resolved.config,
        payload.trackerId,
        payload.repoRoot,
      )
      return result.unwrapOr([])
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
    'taskTracker:buildTaskContext',
    async (event, payload: TaskTrackerBuildTaskContextPayload) => {
      if (
        !payload?.task ||
        typeof payload.task.key !== 'string' ||
        !TASK_KEY_RE.test(payload.task.key)
      ) {
        throw new Error('Invalid task key')
      }
      const repoRoot = payload.repoRoot
        ? await validatePathAccess(event.sender.id, payload.repoRoot)
        : undefined
      const resolved = await resolveEffectiveConfig(repoRoot)
      const hasConfigTracker = Boolean(resolved?.config.trackers.length)

      let fullTask: TrackerTask | null = null
      if (hasConfigTracker && resolved) {
        const result = await taskTrackerManager.findTaskByKeyFromConfig(
          resolved.config,
          payload.task.key,
          payload.trackerId,
          repoRoot,
        )
        if (result.isOk()) fullTask = result.value
      }
      if (!fullTask) {
        fullTask = await taskTrackerManager.findTaskByKey(payload.task.key).catch(() => null)
      }

      let comments: Array<{ author: string; body: string; created: string }> = []
      if (hasConfigTracker && resolved) {
        const result = await taskTrackerManager.fetchTaskCommentsFromConfig(
          resolved.config,
          payload.task.key,
          payload.trackerId,
          repoRoot,
        )
        if (result.isOk()) comments = result.value
      }
      if (comments.length === 0) {
        const result = await taskTrackerManager.fetchTaskComments(
          payload.connectionId,
          payload.task.key,
          repoRoot,
        )
        comments = result.unwrapOr([])
      }

      let rawAttachments: Array<{ name: string; url: string }> = []
      if (hasConfigTracker && resolved) {
        const result = await taskTrackerManager.fetchTaskAttachmentsFromConfig(
          resolved.config,
          payload.task.key,
          payload.trackerId,
          repoRoot,
        )
        if (result.isOk()) rawAttachments = result.value
      }
      if (rawAttachments.length === 0) {
        const result = await taskTrackerManager.fetchTaskAttachments(
          payload.connectionId,
          payload.task.key,
        )
        rawAttachments = result.unwrapOr([])
      }

      const attachments: TaskAttachmentPath[] = []
      const failedAttachments: string[] = []
      const downloadResults = await Promise.allSettled(
        rawAttachments.map(async (attachment) => {
          if (hasConfigTracker && resolved) {
            const configResult = await taskTrackerManager.downloadAttachmentFromConfig(
              resolved.config,
              attachment.url,
              attachment.name,
              payload.trackerId,
              repoRoot,
            )
            if (configResult.isOk()) {
              return { name: attachment.name, localPath: configResult.value }
            }
          }

          const legacyResult = await taskTrackerManager.downloadAttachment(
            payload.connectionId,
            attachment.url,
            attachment.name,
          )
          if (legacyResult.isErr()) {
            throw new Error(taskTrackerErrorMessage(legacyResult.error))
          }
          return { name: attachment.name, localPath: legacyResult.value }
        }),
      )

      for (let i = 0; i < downloadResults.length; i++) {
        const result = downloadResults[i]
        if (result.status === 'fulfilled') {
          attachments.push(result.value)
        } else {
          failedAttachments.push(rawAttachments[i].name)
        }
      }

      if (attachments.length > 0) {
        const paths = attachments.map((attachment) => attachment.localPath)
        setTimeout(() => {
          for (const filePath of paths) {
            taskTrackerManager.cleanupAttachmentDir(filePath)
          }
        }, 60_000)
      }

      const taskForContext: TaskContextInput = fullTask
        ? { ...payload.task, description: fullTask.description || payload.task.description }
        : payload.task

      return formatTaskContext(taskForContext, comments, attachments, failedAttachments)
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
        repoRoot?: string
      },
    ) => {
      return resolveTaskTrackerBranchName(payload)
    },
  )

  ipcMain.handle(
    'taskTracker:prepareBranchFromTask',
    async (event, payload: TaskTrackerBranchFromTaskPayload & { repoRoot: string }) => {
      const repoRoot = await validatePathAccess(event.sender.id, payload.repoRoot)
      const branchName = await resolveTaskTrackerBranchName({ ...payload, repoRoot })
      return { branchName }
    },
  )

  ipcMain.handle(
    'taskTracker:createBranchFromTask',
    async (event, payload: TaskTrackerCreateBranchFromTaskPayload) => {
      const repoRoot = await validatePathAccess(event.sender.id, payload.repoRoot)
      const branchName = await resolveTaskTrackerBranchName({ ...payload, repoRoot })

      if (payload.stashBeforeCreate) {
        const stashResult = await GitRepository.stash(repoRoot)
        unwrapOrThrow(stashResult, gitErrorMessage)
      }

      const createResult = await GitRepository.createBranch(
        repoRoot,
        branchName,
        payload.baseBranch,
      )
      unwrapOrThrow(createResult, gitErrorMessage)

      return { branchName }
    },
  )

  ipcMain.handle(
    'taskTracker:createWorktreeFromTask',
    async (event, payload: TaskTrackerCreateWorktreeFromTaskPayload) => {
      const repoRoot = await validateWorktreeScopedPathAccess(event.sender.id, payload.repoRoot)
      const expandedPath = payload.worktreePath.startsWith('~/')
        ? os.homedir() + payload.worktreePath.slice(1)
        : payload.worktreePath
      const worktreePath = await validateWorktreeCreationPath(event.sender.id, expandedPath)
      const branchName = await resolveTaskTrackerBranchName({ ...payload, repoRoot })
      const result = await GitRepository.worktreeAdd(
        repoRoot,
        worktreePath,
        branchName,
        payload.baseBranch,
      )
      unwrapOrThrow(result, gitErrorMessage)

      return { branchName, worktreePath }
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
    async (event, payload: { repoRoot: string; branch: string }) => {
      // Reject leading-`-` branch names so they can't be consumed as gh flags.
      if (typeof payload.branch !== 'string' || payload.branch.startsWith('-')) return null
      const resolvedRepo = await validatePathAccess(event.sender.id, payload.repoRoot)
      try {
        const { stdout } = await execFileAsync(
          'gh',
          ['pr', 'view', payload.branch, '--json', 'url', '--jq', '.url'],
          { cwd: resolvedRepo },
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

  ipcMain.handle('github:fetchBranchPRs', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validatePathAccess(event.sender.id, payload.repoRoot)
    const found = await gitHubService.findGitHubConnection(resolvedRepo)
    // No connection configured — silent empty return (expected for non-GitHub repos)
    if (found.isErr() || !found.value) return {}
    const { token, repo } = found.value
    const worktrees = await GitRepository.listWorktrees(resolvedRepo).unwrapOr([])
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

  ipcMain.handle('github:getRepoInfo', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validatePathAccess(event.sender.id, payload.repoRoot)
    const found = await gitHubService.findGitHubConnection(resolvedRepo)
    if (found.isErr() || !found.value) return null
    const { token, repo } = found.value
    const result = await gitHubService.getRepoInfo(repo.apiUrl, token, repo.owner, repo.repo)
    return unwrapOrThrow(result, gitHubErrorMessage)
  })

  ipcMain.handle(
    'github:createPR',
    async (
      event,
      payload: {
        repoRoot: string
        title: string
        body: string
        baseRefName: string
        draft: boolean
      },
    ) => {
      const resolvedRepo = await validatePathAccess(event.sender.id, payload.repoRoot)
      if (typeof payload.baseRefName !== 'string' || payload.baseRefName.startsWith('-')) {
        throw new Error('Invalid baseRefName')
      }
      const found = await gitHubService.findGitHubConnection(resolvedRepo)
      if (found.isErr() || !found.value) {
        throw new Error('No GitHub connection found for this repository')
      }
      const { token, repo } = found.value

      const pushResult = await GitRepository.push(resolvedRepo)
      if (pushResult.isErr()) {
        throw new Error(`Failed to push branch: ${gitErrorMessage(pushResult.error)}`)
      }

      const repoInfo = await gitHubService.getRepoInfo(repo.apiUrl, token, repo.owner, repo.repo)
      const repoInfoValue = unwrapOrThrow(repoInfo, gitHubErrorMessage)

      const branch = await GitRepository.getBranch(resolvedRepo).unwrapOr(null)
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

  ipcMain.handle('github:getRepoIdentifier', async (event, payload: { repoRoot: string }) => {
    const resolvedRepo = await validatePathAccess(event.sender.id, payload.repoRoot)
    const result = await gitHubService.getRepoIdentifier(resolvedRepo)
    return result.unwrapOr(null)
  })

  // --- Remote control (WebRTC pairing via QR) ---

  ipcMain.handle('remote:start', async (event, payload?: { interfaceName?: string }) => {
    if (!remoteSessionService.isEnabledInPreferences()) {
      throw new Error('Remote control is disabled in settings')
    }
    const interfaceName =
      typeof payload?.interfaceName === 'string' && payload.interfaceName.length > 0
        ? payload.interfaceName
        : undefined
    // The host webContents owns this session — peer signals are routed back
    // to this window only, not broadcast to the other windows.
    const result = await remoteSessionService.start(event.sender.id, interfaceName)
    return unwrapOrThrow(result, remoteServerErrorMessage)
  })

  ipcMain.handle(
    'remote:ensureListening',
    async (event, payload?: { allowWithoutTrusted?: boolean }) => {
      // Best-effort: auto-bind the signaling server in listen mode. App-mount
      // calls silently no-op without trusted devices; explicit sidebar Listen
      // passes allowWithoutTrusted so first-time pairing can start from the
      // same listening state. Auto-listen errors are swallowed; manual listen
      // returns bind failures to the sidebar.
      const manualListen = payload?.allowWithoutTrusted === true
      const result = await remoteSessionService.ensureListening(event.sender.id, {
        allowWithoutTrusted: manualListen,
      })
      if (manualListen) return unwrapOrThrow(result, remoteServerErrorMessage)
      result.match(
        () => {},
        () => {},
      )
    },
  )

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

  ipcMain.handle('remote:listNetworkInterfaces', () => {
    return listSelectableInterfaces()
  })

  ipcMain.handle('remote:removeTrustedDevice', (_event, payload: { deviceId: string }) => {
    if (!payload || typeof payload.deviceId !== 'string' || payload.deviceId.length === 0) {
      throw new Error('Invalid deviceId')
    }
    remoteSessionService.removeTrustedDevice(payload.deviceId)
  })

  ipcMain.handle(
    'remote:renameTrustedDevice',
    (_event, payload: { deviceId: string; name: string }) => {
      if (!payload || typeof payload.deviceId !== 'string' || payload.deviceId.length === 0) {
        throw new Error('Invalid deviceId')
      }
      if (typeof payload.name !== 'string' || payload.name.trim().length === 0) {
        throw new Error('Device name is required')
      }
      const renamed = remoteSessionService.renameTrustedDevice(payload.deviceId, payload.name)
      if (!renamed) throw new Error('Trusted device not found')
    },
  )

  // --- Run Configurations ---

  ipcMain.handle('runConfig:discover', async (event, payload: { repoRoot: string }) => {
    const resolved = await validatePathAccess(event.sender.id, payload.repoRoot)
    const result = await runConfigManager.discover(resolved)
    return result.unwrapOr([])
  })

  ipcMain.handle(
    'runConfig:save',
    async (event, payload: { configDir: string; config: { configurations: unknown[] } }) => {
      const resolved = await validatePathAccess(event.sender.id, payload.configDir)
      const result = await runConfigManager.saveFile(
        resolved,
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
      const resolved = await validatePathAccess(event.sender.id, payload.configDir)
      const result = await runConfigManager.addConfiguration(resolved, payload.configuration)
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
      const resolved = await validatePathAccess(event.sender.id, payload.configDir)
      const result = await runConfigManager.updateConfiguration(
        resolved,
        payload.name,
        payload.configuration,
      )
      unwrapOrThrow(result, runConfigErrorMessage)
    },
  )

  ipcMain.handle(
    'runConfig:deleteConfig',
    async (event, payload: { configDir: string; name: string }) => {
      const resolved = await validatePathAccess(event.sender.id, payload.configDir)
      const result = await runConfigManager.deleteConfiguration(resolved, payload.name)
      unwrapOrThrow(result, runConfigErrorMessage)
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

  ipcMain.handle('skills:install', async (event, payload: SkillInstallOptions) => {
    // The deploy target comes from the untrusted renderer; confine it to one of
    // this window's attached workspaces before writing skill files into it.
    if (payload.workspacePath) await validatePathAccess(event.sender.id, payload.workspacePath)
    const result = await skillInstaller.install(payload)
    const skill = unwrapOrThrow(result, skillErrorMessage)
    skillRegistry.refresh()
    broadcastSkillsChanged()
    return JSON.parse(JSON.stringify(skill))
  })

  ipcMain.handle(
    'skills:remove',
    async (event, payload: { id: string; workspacePath?: string }) => {
      if (payload.workspacePath) await validatePathAccess(event.sender.id, payload.workspacePath)
      const result = await skillInstaller.remove(payload.id, payload.workspacePath)
      unwrapOrThrow(result, skillErrorMessage)
      skillRegistry.refresh()
      broadcastSkillsChanged()
      return { success: true }
    },
  )

  ipcMain.handle(
    'skills:update',
    async (event, payload: { id: string; workspacePath?: string }) => {
      if (payload.workspacePath) await validatePathAccess(event.sender.id, payload.workspacePath)
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
      event,
      payload: { id: string; agent: string; enabled: boolean; workspacePath?: string },
    ) => {
      if (payload.workspacePath) await validatePathAccess(event.sender.id, payload.workspacePath)
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

  ipcMain.handle('skills:deleteFile', async (event, payload: { filePath: string }) => {
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
    // Scope the delete to the user's home directory (global skills) or one of
    // the window's workspace paths (project-scoped skills). The regex check
    // above alone is too permissive — a path like `/tmp/evil/.claude/skills/x`
    // also contains the segment and would otherwise pass.
    // Refuse if realpath fails for the target: a dangling-symlink fallback to
    // the unresolved path would let a non-existent target slip past the
    // home/workspace check and reach unlink (which would then fail anyway,
    // but the precondition has to hold before we touch the filesystem).
    let resolvedTarget: string
    try {
      resolvedTarget = await fs.promises.realpath(filePath)
    } catch {
      unwrapOrThrow(
        err({
          _tag: 'InvalidSource',
          source: payload.filePath,
          reason: 'Skill file does not exist or cannot be resolved',
        } as SkillError),
        skillErrorMessage,
      )
      return { success: false }
    }
    const homeReal = await fs.promises.realpath(os.homedir()).catch(() => os.homedir())
    const withinHome = resolvedTarget === homeReal || resolvedTarget.startsWith(homeReal + path.sep)
    let withinWorkspace = false
    if (!withinHome) {
      const workspacePaths = windowManager.getWorkspacePaths(event.sender.id)
      for (const wp of workspacePaths) {
        const resolvedWp = await fs.promises.realpath(wp).catch(() => wp)
        if (resolvedTarget === resolvedWp || resolvedTarget.startsWith(resolvedWp + path.sep)) {
          withinWorkspace = true
          break
        }
      }
    }
    if (!withinHome && !withinWorkspace) {
      unwrapOrThrow(
        err({
          _tag: 'InvalidSource',
          source: payload.filePath,
          reason: 'Skill files must live under the user home directory or a workspace path',
        } as SkillError),
        skillErrorMessage,
      )
    }
    // Delete the realpath that was containment-checked above, not the
    // pre-realpath path, so a symlink swap between the check and the unlink
    // cannot redirect the deletion outside home/workspace (TOCTOU).
    await fs.promises.unlink(resolvedTarget)
    return { success: true }
  })
  return {
    grantAttachPath(webContentsId: number, targetPath: string): void {
      workspaceCommandService.grantAttachPath(webContentsId, targetPath)
    },
  }
}
