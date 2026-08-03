import type {
  AgentCommandResult,
  AppStateSnapshot,
  CloseWarningResult,
  CloseWarningTarget,
  EditorFileLoadResult,
  EditorFileSaveResult,
  EditorFileSnapshot,
  RunConfigCommandResult,
  RunConfigProcessSnapshot,
  TabCloseAllPreflightResult,
  TabClosePreflightResult,
  TabCommandResult,
  TabStateSnapshot,
  WorkspaceCommandResult,
} from '../main/commands/types'
import type { CrashReportData } from '../renderer-shared/crashReport'

interface PtyExitData {
  sessionId: string
  exitCode: number
  signal: number
  tmuxSessionName?: string
}

interface TerminalStreamStateChange {
  state: 'paused' | 'resumed'
  reason: 'lock-screen' | 'unlock-screen' | 'suspend' | 'resume'
  pauseReasons: Array<'lock-screen' | 'suspend'>
}

type TerminalStreamState = Omit<TerminalStreamStateChange, 'reason'>

interface PtyStreamDataEvent {
  sessionId: string
  offset: number
  data: string
}

interface PtyStreamClosedEvent {
  sessionId: string
}

interface TmuxSessionInfo {
  name: string
  created: number
  attached: boolean
  cwd: string
}

interface DependencyStatus {
  found: boolean
  path?: string
}

interface DependencyCheckResult {
  results: Record<string, DependencyStatus>
  platform: string
}

interface WorkspaceRow {
  id: string
  path: string
  name: string
  is_git_repo: number
  last_opened: string | null
  cached_branch: string | null
  cached_dirty: number | null
  cached_ahead_behind: string | null
  cached_worktree_count: number | null
}

interface ToolDefinition {
  id: string
  name: string
  command: string
  args: string[]
  icon: string
  category: string
  isCustom: boolean
}

type GitInfo = import('../main/git/GitRepository').GitInfo
type ParsedDiff = import('../main/git/types').ParsedDiff
type GitWorktreeInfo = import('../main/git/GitRepository').GitWorktreeInfo
type GitRefreshFlags = import('../main/git/GitWatcher').GitRefreshFlags

interface WorktreeSetupCommandAction {
  type: 'command'
  command: string
  label?: string
}

interface WorktreeSetupCopyAction {
  type: 'copy'
  source: string
  dest?: string
  label?: string
}

type WorktreeSetupAction = WorktreeSetupCommandAction | WorktreeSetupCopyAction

interface WorktreeSetupProgress {
  actionIndex: number
  totalActions: number
  label: string
  status: 'running' | 'done' | 'error'
  output?: string
  error?: string
  outputChunk?: string
}

interface GitStatus {
  branch: string | null
  isDirty: boolean
  aheadBehind: { ahead: number; behind: number } | null
}

interface GitCommitResult {
  hash: string
  summary: string
}

interface GitPushInfo {
  branch: string
  remote: string
  commitCount: number
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

interface GitBranchList {
  local: string[]
  remote: string[]
  current: string | null
}

interface WorktreeRemoveWithBranchInput {
  repoRoot: string
  worktreePath: string
  branch?: string
  deleteBranch?: boolean
  forceOnFailure?: boolean
}

interface WorktreePrepareRemoveInput {
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

interface WorktreeGetMergedBranchesInput {
  repoRoot: string
  branches: string[]
}

interface WorktreeGetMergedBranchesResult {
  mergedBranches: string[]
}

interface GitBranchPrepareDeleteInput {
  repoRoot: string
  branch: string
}

interface GitBranchPrepareDeleteResult {
  branchMerged: boolean
  forceRequired: boolean
  warnings: string[]
}

interface GitBranchDeleteWithPreflightInput {
  repoRoot: string
  branch: string
  forceIfUnmerged?: boolean
}

interface GitBranchDeleteWithPreflightResult {
  branchDeleted: boolean
  forcedBranchDelete: boolean
  branchMerged: boolean
}

type WorktreeCreateInput =
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
  /** Path left on disk when some files could not be deleted (held by another process). */
  leftoverPath: string | null
}

interface AgentHookEventData {
  ptySessionId: string
  agentType: string
  event: {
    agentType: string
    sessionId: string
    event: string
    rawEventName: string
    toolName?: string
    toolInput?: Record<string, unknown>
    toolResponse?: string
    error?: string
    errorDetails?: string
    message?: string
    title?: string
    notificationType?: string
    agentId?: string
    agentSubtype?: string
    reason?: string
    model?: string
    permissionMode?: string
    extra?: Record<string, unknown>
    [key: string]: unknown
  }
}

interface AgentStatusData {
  ptySessionId: string
  agentType: string
  status: {
    model?: { id?: string; displayName?: string }
    contextWindow?: { usedPercent?: number; size?: number }
    cost?: {
      totalCostUsd?: number
      durationMs?: number
      linesAdded?: number
      linesRemoved?: number
    }
    version?: string
    extra?: Record<string, unknown>
  }
}

interface ChangelogEntry {
  version: string
  date: string
  body: string
}

interface UpdateInfo {
  version: string
  releaseNotes?: string
}

interface UpdateProgress {
  percent: number
  bytesPerSecond: number
  transferred: number
  total: number
}

interface AboutInfo {
  version: string
  homepage: string
  license: string
}

interface BrowserState {
  url: string
  title: string
  canGoBack: boolean
  canGoForward: boolean
  isLoading: boolean
  isDevToolsOpen: boolean
  devToolsMode: 'bottom' | 'left'
}

interface DirEntry {
  name: string
  isDirectory: boolean
  size: number
}

interface FileTreeGitStatus {
  statuses: Record<string, string>
  changedDirs: string[]
  affectedPaths: string[]
}

interface CanopyAPI {
  // About
  getAboutInfo: () => Promise<AboutInfo>
  openExternal: (url: string) => Promise<void>
  openThirdPartyNotices: () => Promise<void>
  quit: () => Promise<void>

  // Agent session
  updateAgentTitle: (sessionId: string, title: string) => Promise<void>

  // Notch overlay
  setNotchEnabled: (enabled: boolean) => void

  // Auto-update
  checkForUpdates: () => Promise<void>
  installUpdate: () => Promise<void>
  setUpdateChannel: (channel: string) => Promise<void>
  setAutoUpdate: (enabled: boolean) => Promise<void>
  setUpdateCheckFrequency: (frequency: string) => Promise<void>
  onUpdateAvailable: (callback: (data: UpdateInfo) => void) => () => void
  onUpdateProgress: (callback: (data: UpdateProgress) => void) => () => void
  onUpdateDownloaded: (callback: (data: UpdateInfo) => void) => () => void
  onUpdateNotAvailable: (callback: () => void) => () => void
  onUpdateError: (callback: (data: { message: string }) => void) => () => void
  onUpdateInstalling: (callback: () => void) => () => void

  // Onboarding
  getOnboardingCompleted: () => Promise<string[]>
  completeOnboarding: (stepIds: string[], appVersion: string) => Promise<void>
  resetOnboarding: () => Promise<void>
  onShowOnboarding: (
    callback: (data: { mode: 'first-launch' | 'upgrade'; fromVersion?: string }) => void,
  ) => () => void

  // Changelog
  getChangelogSinceVersion: (fromVersion: string) => Promise<ChangelogEntry[] | null>
  onShowChangelog: (callback: (data: { fromVersion: string }) => void) => () => void

  // Crash reports
  onCrashReport: (callback: (data: CrashReportData) => void) => () => void

  // PTY
  resizePty: (sessionId: string, cols: number, rows: number) => Promise<void>
  killPty: (sessionId: string, killTmux?: boolean) => Promise<void>
  writePty: (sessionId: string, data: string) => Promise<void>
  getPtyDimensions: (sessionId: string) => Promise<{ cols: number; rows: number } | null>
  hasPtyStream: (sessionId: string) => Promise<boolean>
  subscribePtyData: (
    sessionId: string,
    offset: number,
    callback: (event: PtyStreamDataEvent) => void,
    onClose?: (event: PtyStreamClosedEvent) => void,
    onError?: (error: unknown) => void,
  ) => () => void
  getTerminalStreamState: () => Promise<TerminalStreamState>
  onTerminalStreamStateChanged: (callback: (data: TerminalStreamStateChange) => void) => () => void

  // Tmux
  tmuxIsAvailable: () => Promise<boolean>
  tmuxGetVersion: () => Promise<string | null>
  tmuxListSessions: () => Promise<TmuxSessionInfo[]>
  tmuxHasSession: (name: string) => Promise<boolean>
  tmuxAttach: (
    tmuxSessionName: string,
    options?: { cols?: number; rows?: number },
  ) => Promise<{ sessionId: string; wsUrl: string }>
  tmuxDetach: (sessionId: string) => Promise<{ tmuxSessionName?: string }>
  tmuxKillSession: (name: string) => Promise<void>
  tmuxRenameSession: (oldName: string, newName: string) => Promise<void>

  // Workspaces
  listWorkspaces: (limit?: number) => Promise<WorkspaceRow[]>
  getWorkspace: (id: string) => Promise<WorkspaceRow | null>
  getWorkspaceByPath: (path: string) => Promise<WorkspaceRow | null>
  removeWorkspace: (id: string) => Promise<void>
  workspaceRestoreWindow: (payload: {
    paths: string[]
    activeWorktreePath?: string
    removedPaths?: string[]
  }) => Promise<WorkspaceCommandResult>
  workspaceAttachProject: (path: string) => Promise<WorkspaceCommandResult>
  workspaceDetachProject: (path: string) => Promise<WorkspaceCommandResult>
  workspaceSelectWorktree: (path: string) => Promise<WorkspaceCommandResult>
  workspaceInitGitRepo: (path: string) => Promise<WorkspaceCommandResult>
  getAppState: () => Promise<AppStateSnapshot>
  getStartupRestoreState: () => Promise<{ restoring: boolean }>
  completeStartupRestore: () => Promise<void>
  onAppStateChanged: (callback: (snapshot: AppStateSnapshot) => void) => () => void
  fileTreeReadDir: (dirPath: string) => Promise<DirEntry[]>
  fileTreeCreateFile: (filePath: string) => Promise<void>
  fileTreeCreateDirectory: (dirPath: string) => Promise<void>
  fileTreeGetGitStatus: (repoRoot: string, worktreePath: string) => Promise<FileTreeGitStatus>

  // Preferences
  getPref: (key: string) => Promise<string | null>
  setPref: (key: string, value: string) => Promise<void>
  getAllPrefs: () => Promise<Record<string, string>>
  deletePref: (key: string) => Promise<void>

  // Environment / Dependencies
  checkDependencies: (tools: string[]) => Promise<DependencyCheckResult>

  // Tools
  listTools: () => Promise<ToolDefinition[]>
  getTool: (id: string) => Promise<ToolDefinition | null>
  checkToolAvailability: () => Promise<Record<string, boolean>>
  addCustomTool: (tool: {
    id: string
    name: string
    command: string
    args?: string[]
    icon?: string
    category?: string
  }) => Promise<ToolDefinition[]>
  removeCustomTool: (id: string) => Promise<ToolDefinition[]>
  updateCustomTool: (
    id: string,
    changes: {
      name?: string
      command?: string
      args?: string[]
      icon?: string
      category?: string
    },
  ) => Promise<ToolDefinition[]>
  tabOpenTool: (
    toolId: string,
    worktreePath: string,
    options?: {
      initialUrl?: string
      profileId?: string
      workspaceName?: string
      branch?: string
    },
  ) => Promise<TabCommandResult>
  tabOpenDiff: (worktreePath: string) => Promise<TabCommandResult>
  tabOpenSessionTab: (
    worktreePath: string,
    name: string,
    sessionId: string,
  ) => Promise<TabCommandResult>
  tabOpenEditorFile: (worktreePath: string, filePath: string) => Promise<TabCommandResult>
  tabDetachEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
  ) => Promise<TabCommandResult>
  tabCloseEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
  ) => Promise<TabCommandResult>
  tabPrepareCloseEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
  ) => Promise<TabClosePreflightResult>
  tabMoveEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    toIndex: number,
  ) => Promise<TabCommandResult>
  tabMoveEditorFileBetweenPanes: (
    worktreePath: string,
    sourcePaneId: string,
    targetPaneId: string,
    filePath: string,
    toIndex: number,
  ) => Promise<TabCommandResult>
  tabSetActiveEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
  ) => Promise<TabCommandResult>
  tabUpdateEditorFileState: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    patch: Partial<EditorFileSnapshot>,
  ) => Promise<TabCommandResult>
  tabLoadEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    options?: { maxBytes?: number },
  ) => Promise<EditorFileLoadResult>
  tabSaveEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    options: {
      content: string
      fileLineEnding?: 'LF' | 'CRLF'
      expectedMtimeMs?: number
    },
  ) => Promise<EditorFileSaveResult>
  tabUpdatePaneTitle: (
    worktreePath: string,
    sessionId: string,
    title: string,
  ) => Promise<TabCommandResult>
  tabUpdatePaneUrl: (
    worktreePath: string,
    sessionId: string,
    url: string,
  ) => Promise<TabCommandResult>
  tabUpdateTmuxSessionName: (
    worktreePath: string,
    oldName: string,
    newName: string,
  ) => Promise<TabCommandResult>
  tabHandlePtyExit: (
    worktreePath: string,
    sessionId: string,
    exitCode: number,
    tmuxSessionName?: string,
  ) => Promise<TabCommandResult>
  tabKillTmuxPane: (
    worktreePath: string,
    tabId: string,
    paneId: string,
  ) => Promise<TabCommandResult>
  tabReattachTmuxPane: (
    worktreePath: string,
    tabId: string,
    paneId: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) => Promise<TabCommandResult>
  tabToggleFocusedInspector: (worktreePath: string, tabId: string) => Promise<TabCommandResult>
  tabRestartPane: (
    worktreePath: string,
    tabId: string,
    paneId: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) => Promise<TabCommandResult>
  tabCloseTab: (worktreePath: string, tabId: string) => Promise<TabCommandResult>
  tabPrepareCloseTab: (worktreePath: string, tabId: string) => Promise<TabClosePreflightResult>
  tabPrepareCloseAllForWorktree: (
    worktreePath: string,
    options?: { confirmedActiveProcesses?: boolean },
  ) => Promise<TabCloseAllPreflightResult>
  tabGetCloseWarning: (
    worktreePath: string,
    target: CloseWarningTarget,
  ) => Promise<CloseWarningResult>
  tabReopenClosedTab: (
    worktreePath: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) => Promise<TabCommandResult>
  tabClosePane: (worktreePath: string, tabId: string, paneId: string) => Promise<TabCommandResult>
  tabCloseAllForWorktree: (worktreePath: string, forRemoval?: boolean) => Promise<TabCommandResult>
  tabSetActiveTab: (worktreePath: string, tabId: string) => Promise<TabCommandResult>
  tabMoveTab: (
    worktreePath: string,
    fromIndex: number,
    toIndex: number,
  ) => Promise<TabCommandResult>
  tabMoveTabToSplit: (
    worktreePath: string,
    sourceTabId: string,
    targetTabId: string,
    targetPaneId: string,
    direction: 'horizontal' | 'vertical',
    position: 'first' | 'second',
  ) => Promise<TabCommandResult>
  tabMovePaneToTarget: (
    worktreePath: string,
    sourceTabId: string,
    sourcePaneId: string,
    targetTabId: string,
    targetPaneId: string,
    direction: 'horizontal' | 'vertical',
    position: 'first' | 'second',
  ) => Promise<TabCommandResult>
  tabDetachPaneToTab: (
    worktreePath: string,
    sourceTabId: string,
    sourcePaneId: string,
  ) => Promise<TabCommandResult>
  tabSpawnPane: (
    toolId: string,
    worktreePath: string,
    options?: {
      initialUrl?: string
      profileId?: string
      workspaceName?: string
      branch?: string
      resumeSessionId?: string
    },
  ) => Promise<PaneSnapshot>
  tabSplitPane: (
    worktreePath: string,
    tabId: string,
    paneId: string,
    direction: 'horizontal' | 'vertical',
  ) => Promise<TabCommandResult>
  tabFocusPane: (worktreePath: string, tabId: string, paneId: string) => Promise<TabCommandResult>
  tabNavigatePaneFocus: (
    worktreePath: string,
    tabId: string,
    direction: 'left' | 'right' | 'up' | 'down',
  ) => Promise<TabCommandResult>
  tabUpdateSplitRatio: (
    worktreePath: string,
    tabId: string,
    splitId: string,
    ratio: number,
  ) => Promise<TabCommandResult>
  tabRestoreLayout: (
    worktreePath: string,
    layoutJson: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) => Promise<TabCommandResult & { restored: boolean }>
  tabResumeSuspendedTab: (
    worktreePath: string,
    tabId: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) => Promise<TabCommandResult>
  tabKillAll: () => Promise<TabStateSnapshot>
  tabFocusSession: (sessionId: string) => Promise<TabCommandResult | null>
  tabSaveCurrentLayout: (worktreePath: string) => Promise<void>

  agentSendTaskContext: (payload: {
    text: string
    worktreePath?: string
    sessionId?: string
  }) => Promise<AgentCommandResult>
  agentSendReviewContext: (payload: {
    text: string
    worktreePath?: string
    sessionId?: string
  }) => Promise<AgentCommandResult>
  agentSendDrawing: (payload: {
    worktreePath?: string
    sessionId?: string
  }) => Promise<AgentCommandResult>

  // App / Shell
  isCredentialEncryptionAvailable: () => Promise<boolean>
  showInFolder: (path: string) => Promise<void>
  newWindow: () => Promise<void>
  focusRendererWebContents: () => Promise<void>
  setFocusedAgentSession: (ptySessionId: string | null) => Promise<void>

  // Dialog
  openFolder: (defaultPath?: string) => Promise<string | null>
  confirmOpenPath: (path: string) => Promise<string | null>

  // Settings export / import
  exportSettings: () => Promise<{
    path: string
    counts: {
      preferences: number
      profiles: number
      credentials: number
      customTools: number
    }
  } | null>
  importSettings: () => Promise<{
    counts: {
      preferences: number
      profiles: number
      credentials: number
      customTools: number
    }
  } | null>

  // Workspace Git Status
  refreshWorkspaceGitStatus: (id: string, path: string) => Promise<WorkspaceRow | null>

  // Git
  gitDetect: (path: string) => Promise<GitInfo>
  gitWorktrees: (repoRoot: string) => Promise<GitWorktreeInfo[]>
  gitStatus: (path: string) => Promise<GitStatus>
  gitWatch: (repoRoot: string, snapshot?: GitInfo) => Promise<void>
  gitUnwatch: (repoRoot?: string) => Promise<void>
  gitInit: (path: string) => Promise<GitInfo>

  // File Tree Watcher
  watchFiles: (repoRoot: string) => Promise<void>
  unwatchFiles: () => Promise<void>
  updateFileIgnorePatterns: (patterns: string[]) => Promise<void>
  getDefaultFileIgnorePatterns: () => Promise<string[]>

  // Git Operations
  gitCommit: (repoRoot: string, message: string, stageAll?: boolean) => Promise<GitCommitResult>
  gitPush: (repoRoot: string) => Promise<{ branch: string; remote: string }>
  gitPull: (repoRoot: string, rebase: boolean) => Promise<{ summary: string }>
  gitPullWithPreferences: (payload: { repoRoot: string }) => Promise<{
    summary: string
    rebase: boolean
  }>
  gitCommitWorktree: (payload: {
    repoRoot: string
    message: string
    stageAll?: boolean
  }) => Promise<GitCommitResult>
  gitPushWorktree: (payload: { repoRoot: string }) => Promise<{ branch: string; remote: string }>
  gitFetchWorktree: (payload: { repoRoot: string }) => Promise<void>
  gitFetch: (repoRoot: string) => Promise<void>
  gitFetchAll: (repoRoot: string) => Promise<void>
  gitStashWorktree: (payload: { repoRoot: string }) => Promise<void>
  gitStash: (repoRoot: string) => Promise<void>
  gitStashPopWorktree: (payload: { repoRoot: string }) => Promise<void>
  gitStashPop: (repoRoot: string) => Promise<void>
  gitBranches: (repoRoot: string) => Promise<GitBranchList>
  gitBranchCreate: (repoRoot: string, name: string, baseBranch: string) => Promise<void>
  gitBranchCreateFromHead: (payload: { repoRoot: string; branch: string }) => Promise<void>
  gitCheckout: (repoRoot: string, branch: string) => Promise<void>
  gitBranchDelete: (repoRoot: string, name: string, force: boolean) => Promise<void>
  gitBranchDeleteRemote: (repoRoot: string, remote: string, name: string) => Promise<void>
  gitPushInfo: (repoRoot: string) => Promise<GitPushInfo | null>
  gitPreparePush: (payload: { repoRoot: string }) => Promise<GitPreparePushResult>
  gitBranchMerged: (repoRoot: string, branch: string) => Promise<boolean>
  gitWorktreeAdd: (
    repoRoot: string,
    path: string,
    branch: string,
    baseBranch: string,
  ) => Promise<void>
  gitWorktreeCheckout: (
    repoRoot: string,
    path: string,
    branch: string,
    createLocalTracking: boolean,
  ) => Promise<void>
  gitWorktreeRemove: (repoRoot: string, path: string, force: boolean) => Promise<void>
  worktreeCreate: (payload: WorktreeCreateInput) => Promise<WorktreeCreateResult>
  worktreePrepareRemove: (
    payload: WorktreePrepareRemoveInput,
  ) => Promise<WorktreePrepareRemoveResult>
  worktreeGetMergedBranches: (
    payload: WorktreeGetMergedBranchesInput,
  ) => Promise<WorktreeGetMergedBranchesResult>
  worktreeListBranches: (payload: { repoRoot: string }) => Promise<GitBranchList>
  worktreeRefreshBranches: (payload: { repoRoot: string }) => Promise<GitBranchList>
  gitBranchPrepareDelete: (
    payload: GitBranchPrepareDeleteInput,
  ) => Promise<GitBranchPrepareDeleteResult>
  gitBranchDeleteWithPreflight: (
    payload: GitBranchDeleteWithPreflightInput,
  ) => Promise<GitBranchDeleteWithPreflightResult>
  worktreeRemoveWithBranch: (
    payload: WorktreeRemoveWithBranchInput,
  ) => Promise<WorktreeRemoveWithBranchResult>
  gitUnmergedCommits: (repoRoot: string, branch: string) => Promise<string[]>
  gitStatusPorcelain: (repoRoot: string, worktreePath?: string) => Promise<string>
  changesGetDiff: (payload: { worktreePath: string }) => Promise<ParsedDiff>
  changesStageFile: (payload: { worktreePath: string; filePath: string }) => Promise<void>
  changesRevertFile: (payload: { worktreePath: string; filePath: string }) => Promise<void>
  gitDiff: (repoRoot: string) => Promise<ParsedDiff>
  gitDiffFile: (repoRoot: string, filePath: string) => Promise<ParsedDiff>
  gitStageFile: (repoRoot: string, filePath: string) => Promise<void>
  gitRevertFile: (repoRoot: string, filePath: string) => Promise<void>
  gitGenerateCommitMessage: (repoRoot: string) => Promise<string | null>

  // Browser (<webview> management)
  setupBrowserWebview: (browserId: string, webContentsId: number) => Promise<void>
  teardownBrowserWebview: (browserId: string) => Promise<void>
  openBrowserDevTools: (browserId: string) => Promise<void>
  closeBrowserDevTools: (browserId: string) => Promise<void>
  setBrowserDevToolsBounds: (
    browserId: string,
    bounds: { x: number; y: number; width: number; height: number },
  ) => Promise<void>
  setBrowserDeviceEmulation: (
    browserId: string,
    device: { width: number; height: number; scaleFactor: number; mobile: boolean } | null,
  ) => Promise<void>
  setBrowserBackgroundThrottling: (browserId: string, allowed: boolean) => Promise<void>
  saveBrowserCapture: (buffer: ArrayBuffer) => Promise<string>

  // Credential autofill (isolated world)
  fillBrowserCredential: (browserId: string, username: string, password: string) => Promise<void>

  // Credentials
  getCredentials: (
    domain: string,
  ) => Promise<Array<{ id: string; domain: string; username: string; title: string }>>
  saveCredential: (
    domain: string,
    username: string,
    password: string,
    title?: string,
  ) => Promise<void>
  getCredentialDecrypted: (
    id: string,
    domain: string,
    purpose: 'autofill' | 'reveal',
  ) => Promise<{ id: string; username: string; password: string } | null>
  deleteCredential: (id: string) => Promise<void>
  listCredentials: () => Promise<
    Array<{
      id: string
      domain: string
      username: string
      title: string
      createdAt: string
      updatedAt: string
    }>
  >

  // Browser push events (main → renderer)
  onBrowserFaviconChanged: (
    callback: (data: { browserId: string; favicon: string | null }) => void,
  ) => () => void
  onBrowserDevToolsOpened: (callback: (data: { browserId: string }) => void) => () => void
  onBrowserFocused: (callback: (data: { browserId: string }) => void) => () => void
  onBrowserOpenUrl: (callback: (data: { browserId: string; url: string }) => void) => () => void

  // Worktree Setup
  runWorktreeSetup: (
    workspaceId: string,
    repoRoot: string,
    newWorktreePath: string,
  ) => Promise<{ success: boolean; errors: string[] }>
  abortWorktreeSetup: () => void

  // Push events (main → renderer)
  onAgentHookEvent: (callback: (data: AgentHookEventData) => void) => () => void
  onAgentStatusUpdate: (callback: (data: AgentStatusData) => void) => () => void
  onAgentFocusSession: (callback: (data: { ptySessionId: string }) => void) => () => void
  onGitChanged: (
    callback: (
      info: GitInfo & {
        repoRoot: string
        changes: GitRefreshFlags
      },
    ) => void,
  ) => () => void
  onFilesChanged: (
    callback: (payload: {
      repoRoot: string
      events: { type: 'add' | 'change' | 'unlink'; path: string }[]
    }) => void,
  ) => () => void
  onToolsChanged: (callback: (tools: ToolDefinition[]) => void) => () => void
  onSkillsChanged: (
    callback: (skills: import('../main/skills/types').CanopySkill[]) => void,
  ) => () => void

  // Skills
  listSkills: (opts?: {
    scope?: string
    agent?: string
    workspaceId?: string | null
  }) => Promise<unknown[]>
  getSkill: (id: string) => Promise<unknown | null>
  installSkill: (opts: {
    source: string
    agents?: string[]
    scope?: string
    method?: string
    workspaceId?: string | null
    workspacePath?: string
  }) => Promise<unknown>
  removeSkill: (id: string, workspacePath?: string) => Promise<void>
  updateSkill: (id: string, workspacePath?: string) => Promise<unknown>
  toggleSkillAgent: (
    id: string,
    agent: string,
    enabled: boolean,
    workspacePath?: string,
  ) => Promise<unknown>
  scanSkills: (workspacePath?: string) => Promise<void>
  deleteSkillFile: (filePath: string) => Promise<{ success: boolean }>
  onPtyExit: (callback: (data: PtyExitData) => void) => () => void
  onPtyResized: (callback: (sessionId: string, cols: number, rows: number) => void) => () => void
  onWorktreeSetupProgress: (callback: (data: WorktreeSetupProgress) => void) => () => void
  onUrlAction: (
    callback: (data: { action: string; path: string; tool?: string; worktree?: string }) => void,
  ) => () => void
  onRestoreWindow: (
    callback: (data: {
      paths: string[]
      activeWorktreePath?: string
      removedPaths?: string[]
    }) => void,
  ) => () => void

  // Menu events
  onMenuShowAbout: (callback: () => void) => () => void
  onMenuShowPreferences: (callback: () => void) => () => void

  // Filesystem
  quickOpenListFiles: (worktreePath: string, force?: boolean) => Promise<string[]>
  quickOpenInvalidateCache: (worktreePath: string) => Promise<void>

  // Repo Config
  repoConfigLoad: (repoRoot: string) => Promise<RepoConfig | null>
  repoConfigSave: (repoRoot: string, config: RepoConfig) => Promise<void>
  repoConfigExists: (repoRoot: string) => Promise<boolean>
  repoConfigInit: (repoRoot: string) => Promise<RepoConfig>

  // Global Config
  globalConfigLoad: () => Promise<RepoConfig | null>
  globalConfigSave: (config: RepoConfig) => Promise<void>
  globalConfigExists: () => Promise<boolean>

  // Resolved Config (merged global + repo)
  trackerResolvedConfig: (repoRoot?: string) => Promise<ResolvedConfig | null>

  // Config-based tracker methods
  trackerConfigFetchBoards: (repoRoot?: string, trackerId?: string) => Promise<TrackerBoard[]>
  trackerConfigFetchProjects: (
    repoRoot?: string,
    trackerId?: string,
    all?: boolean,
  ) => Promise<Array<{ key: string; name: string }>>
  trackerConfigFetchTaskTypes: (repoRoot?: string, trackerId?: string) => Promise<string[]>
  trackerConfigFetchStatuses: (
    repoRoot?: string,
    trackerId?: string,
    boardId?: string,
  ) => Promise<TrackerStatus[]>
  trackerConfigFetchTasks: (
    repoRoot?: string,
    trackerId?: string,
    params?: { statuses?: string[]; assignedToMe?: boolean; boardId?: string; projectKey?: string },
  ) => Promise<TrackerTask[]>
  trackerConfigGetCurrentUser: (repoRoot?: string, trackerId?: string) => Promise<string>
  trackerConfigFetchTaskComments: (
    repoRoot: string | undefined,
    taskKey: string,
    trackerId?: string,
  ) => Promise<TrackerComment[]>
  trackerConfigFetchTransitions: (
    repoRoot: string | undefined,
    taskKey: string,
    trackerId?: string,
  ) => Promise<TrackerTransition[]>
  trackerConfigApplyTransition: (payload: {
    repoRoot?: string
    trackerId?: string
    taskKey: string
    transitionId: string
    fields?: Record<string, string>
    comment?: string
  }) => Promise<void>
  trackerConfigAddComment: (payload: {
    repoRoot?: string
    trackerId?: string
    taskKey: string
    body: string
  }) => Promise<void>
  trackerConfigFetchTaskAttachments: (
    repoRoot: string | undefined,
    taskKey: string,
    trackerId?: string,
  ) => Promise<TrackerAttachment[]>
  trackerConfigDownloadAttachment: (
    repoRoot: string | undefined,
    url: string,
    filename: string,
    trackerId?: string,
  ) => Promise<string>
  trackerConfigFindTaskByKey: (
    repoRoot: string | undefined,
    taskKey: string,
    trackerId?: string,
  ) => Promise<TrackerTask | null>
  trackerConfigFetchAssignableUsers: (
    repoRoot: string | undefined,
    trackerId?: string,
    projectKey?: string,
  ) => Promise<TrackerUser[]>
  trackerConfigFetchSprints: (
    repoRoot: string | undefined,
    trackerId?: string,
    boardId?: string,
  ) => Promise<TrackerSprint[]>
  trackerConfigFetchCreateTaskTypes: (
    repoRoot: string | undefined,
    trackerId?: string,
    projectKey?: string,
  ) => Promise<Array<{ name: string; iconUrl?: string }>>
  trackerConfigCreateTask: (payload: {
    repoRoot?: string
    trackerId?: string
    projectKey?: string
    typeName?: string
    title: string
    description?: string
    assigneeId?: string
    boardId?: string
    sprintId?: string
    attachments?: Array<{ filename: string; mimeType: string; dataBase64: string }>
  }) => Promise<CreatedTask>

  // Keychain
  keychainHasCredentials: (provider: string, baseUrl: string) => Promise<boolean>
  keychainSetCredentials: (
    provider: string,
    baseUrl: string,
    token: string,
    username?: string,
  ) => Promise<void>
  keychainDeleteCredentials: (provider: string, baseUrl: string) => Promise<void>
  keychainGetCredentials: (
    provider: string,
    baseUrl: string,
  ) => Promise<{ username?: string; hasToken: boolean } | null>
  keychainListCredentials: () => Promise<
    Array<{ provider: string; baseUrl: string; username?: string }>
  >

  // CI (TeamCity)
  ciConfig: (repoRoot: string) => Promise<CiConfigResult>
  ciStatus: (repoRoot: string, branch: string) => Promise<CiStatusResponse>
  ciTrigger: (
    repoRoot: string,
    buildTypeId: string,
    branch: string,
    properties?: Array<{ name: string; value: string }>,
  ) => Promise<CiTriggerResult>
  ciBuild: (repoRoot: string, buildId: number) => Promise<CiBuildStatus>
  ciTestNewConnection: (baseUrl: string, token: string) => Promise<void>
  ciBuildParameters: (repoRoot: string, buildTypeId: string) => Promise<CiParameter[]>
  ciActivity: (repoRoot: string) => Promise<CiActivity>
  ciBranches: (repoRoot: string, buildTypeId: string) => Promise<string[]>
  ciListBuildTypes: (baseUrl: string) => Promise<CiServerBuildType[]>
  ciSaveConfig: (
    repoRoot: string,
    ci: { baseUrl: string; buildTypes: Array<{ id: string; label: string }> } | null,
  ) => Promise<void>

  // Task Tracker
  taskTrackerGetConnections: () => Promise<TaskTrackerConnectionInfo[]>
  taskTrackerAddConnection: (connection: {
    provider: TaskTrackerProvider
    name: string
    baseUrl: string
    projectKey: string
    boardId?: string
    username?: string
    token: string
  }) => Promise<TaskTrackerConnectionInfo>
  taskTrackerRemoveConnection: (connectionId: string) => Promise<void>
  taskTrackerUpdateConnection: (
    connectionId: string,
    updates: { name?: string; baseUrl?: string; username?: string; token?: string },
  ) => Promise<TaskTrackerConnectionInfo | null>
  taskTrackerTestConnection: (connectionId: string) => Promise<boolean>
  taskTrackerTestNewConnection: (connection: {
    provider: TaskTrackerProvider
    name: string
    baseUrl: string
    projectKey: string
    boardId?: string
    username?: string
    token: string
  }) => Promise<boolean>
  taskTrackerFetchBoards: (connectionId: string, repoRoot?: string) => Promise<TrackerBoard[]>
  taskTrackerFetchBoardsForNew: (connection: {
    provider: TaskTrackerProvider
    name: string
    baseUrl: string
    projectKey?: string
    username?: string
    token: string
  }) => Promise<TrackerBoard[]>
  taskTrackerFetchStatuses: (
    connectionId: string,
    boardId?: string,
    repoRoot?: string,
  ) => Promise<TrackerStatus[]>
  taskTrackerFetchTasks: (
    connectionId: string,
    params: {
      statuses?: string[]
      assignedToMe?: boolean
      boardId?: string
      repoRoot?: string
    },
  ) => Promise<TrackerTask[]>
  taskTrackerGetCurrentSprint: (
    connectionId: string,
    boardId?: string,
    repoRoot?: string,
  ) => Promise<TrackerSprint | null>
  taskTrackerGetCurrentUser: (connectionId: string) => Promise<string>
  taskTrackerFetchTaskComments: (
    connectionId: string,
    taskKey: string,
    repoRoot?: string,
  ) => Promise<Array<{ id: string; author: string; body: string; created: string }>>
  taskTrackerFetchTaskAttachments: (
    connectionId: string,
    taskKey: string,
  ) => Promise<Array<{ id: string; name: string; mimeType: string; size: number; url: string }>>
  taskTrackerDownloadAttachment: (
    connectionId: string,
    url: string,
    filename: string,
  ) => Promise<string>
  taskTrackerCleanupAttachments: (filePaths: string[]) => Promise<void>
  taskTrackerBuildTaskContext: (payload: TaskTrackerBuildTaskContextInput) => Promise<string>
  taskTrackerResolveBranchName: (
    connectionId: string,
    task: TrackerTask,
    boardId?: string,
    branchType?: string,
    repoRoot?: string,
  ) => Promise<string>
  taskTrackerPrepareBranchFromTask: (
    payload: TaskTrackerBranchFromTaskInput,
  ) => Promise<TaskTrackerBranchFromTaskResult>
  taskTrackerCreateBranchFromTask: (
    payload: TaskTrackerCreateBranchFromTaskInput,
  ) => Promise<TaskTrackerBranchFromTaskResult>
  taskTrackerCreateWorktreeFromTask: (
    payload: TaskTrackerCreateWorktreeFromTaskInput,
  ) => Promise<TaskTrackerCreateWorktreeFromTaskResult>
  taskTrackerResolveBranchType: (
    taskType: string,
    connectionId?: string,
    boardId?: string,
    repoRoot?: string,
  ) => Promise<{
    defaultType: string
    options: string[]
    hasBranchType: boolean
  }>
  taskTrackerRenderBranchPreview: (
    template: string,
    customVars?: Record<string, string>,
  ) => Promise<string>
  taskTrackerGetAvailablePlaceholders: (
    customVars?: Record<string, string>,
  ) => Promise<Array<{ key: string; description: string; example: string }>>
  taskTrackerValidateTemplate: (template: string) => Promise<{ valid: boolean; errors: string[] }>
  taskTrackerFindTaskByKey: (taskKey: string) => Promise<TrackerTask | null>
  taskTrackerResolvePRPreview: (
    taskKey: string,
    connectionId?: string,
    boardId?: string,
    repoRoot?: string,
  ) => Promise<{ title: string; targetBranch: string }>
  taskTrackerCreatePR: (
    repoRoot: string,
    task: TrackerTask,
    sourceBranch: string,
    connectionId?: string,
    overrides?: {
      title?: string
      body?: string
      targetBranch?: string
      reviewers?: string[]
      assignees?: string[]
    },
  ) => Promise<{ url: string; title: string; targetBranch: string }>

  taskTrackerPreparePR: (
    repoRoot: string,
    task?: { key: string; [k: string]: unknown },
    branch?: string,
  ) => Promise<{
    title: string
    body: string
    targetBranch: string
    repo: string
    task: TrackerTask | null
    branches: string[]
    users: string[]
    viewer: string
    titleTemplate: string
  }>

  taskTrackerFindPR: (repoRoot: string, branch: string) => Promise<string | null>

  taskTrackerPRDetails: (
    repoRoot: string,
    branch: string,
  ) => Promise<{
    number: number
    title: string
    state: string
    url: string
    body: string
    baseRefName: string
    headRefName: string
    isDraft: boolean
    reviewDecision: string | null
    author?: { login?: string; name?: string }
    createdAt?: string
    additions?: number
    deletions?: number
    changedFiles?: number
    statusCheckRollup?: Array<{ status?: string; conclusion?: string; state?: string }>
    mergedAt?: string | null
    closedAt?: string | null
    mergedBy?: { login?: string; name?: string } | null
    mergeable?: string
    mergeStateStatus?: string
    assignees?: Array<{ login?: string; name?: string }>
    reviewRequests?: Array<{ login?: string; name?: string; slug?: string }>
    latestReviews?: Array<{ author?: { login?: string }; state?: string }>
  } | null>

  taskTrackerPRMerge: (
    repoRoot: string,
    prNumber: number,
    strategy: 'merge' | 'squash' | 'rebase',
    deleteBranch?: boolean,
  ) => Promise<void>
  taskTrackerPRClose: (repoRoot: string, prNumber: number, deleteBranch?: boolean) => Promise<void>
  taskTrackerPRDeleteBranch: (repoRoot: string, branch: string) => Promise<void>
  taskTrackerRemoteBranchExists: (repoRoot: string, branch: string) => Promise<boolean>
  taskTrackerSaveAgentImage: (bytes: ArrayBuffer) => Promise<string>
  taskTrackerImageAsDataUrl: (
    repoRoot: string | undefined,
    url: string,
    trackerId?: string,
  ) => Promise<string | null>
  trackerConfigAttachmentPreview: (
    repoRoot: string | undefined,
    taskKey: string,
    attachmentId: string,
    trackerId?: string,
  ) => Promise<string>
  /** Save an attachment to disk via a native save dialog; resolves with the saved
   *  path, or null when the user cancelled. */
  trackerConfigAttachmentSave: (
    repoRoot: string | undefined,
    taskKey: string,
    attachmentId: string,
    trackerId?: string,
  ) => Promise<string | null>

  // GitHub PR features
  githubFetchBranchPRs: (repoRoot: string) => Promise<GitHubBranchPRMap>
  githubGetRepoInfo: (repoRoot: string) => Promise<GitHubRepoInfo | null>
  githubCreatePR: (
    repoRoot: string,
    params: { title: string; body: string; baseRefName: string; draft: boolean },
  ) => Promise<GitHubPRInfo>
  githubGetRepoIdentifier: (
    repoRoot: string,
  ) => Promise<{ owner: string; repo: string; host: string; apiUrl: string } | null>

  // Performance diagnostics (only present when CANOPY_PERF=1)
  perfDiagnostics?: () => Promise<{
    ptySessionCount: number
    terminalStreamCount: number
    terminalStreamSubscriberCount: number
    agentSessionCount: number
    gitWatcherCount: number
    windowCount: number
    heapUsed: number
    rss: number
    uptime: number
    marks: Array<{ name: string; startTime: number }>
  } | null>
  perfIpcLog?: () => Promise<Array<{
    channel: string
    size: number
    ts: number
    dir: string
  }> | null>
  perfDisconnectTerminalClients?: () => Promise<number>
  perfOpenProject?: (path: string) => Promise<void>

  // Status-bar perf HUD (always present)
  perfHud: {
    start: () => Promise<void>
    stop: () => Promise<void>
    onMetrics: (callback: (metrics: { cpu: number; memMb: number }) => void) => () => void
  }

  // Remote control (WebRTC pairing via QR)
  remote: RemoteAPI

  // File utilities
  getPathForFile: (file: File) => string

  // Platform
  platform: NodeJS.Platform

  // Agent Profiles
  listProfiles: (agentType?: AgentType) => Promise<AgentProfileMasked[]>
  getProfile: (id: string) => Promise<AgentProfileMasked | null>
  saveProfile: (input: ProfileInput) => Promise<AgentProfileMasked>
  deleteProfile: (id: string) => Promise<void>
  onProfilesChanged: (callback: (profiles: AgentProfileMasked[]) => void) => () => void

  // Run Configurations
  runConfigDiscover: (repoRoot: string) => Promise<RunConfigSource[]>
  runConfigSave: (configDir: string, config: RunConfigFile) => Promise<void>
  runConfigAddConfig: (configDir: string, configuration: RunConfiguration) => Promise<void>
  runConfigUpdateConfig: (
    configDir: string,
    name: string,
    configuration: RunConfiguration,
  ) => Promise<void>
  runConfigDeleteConfig: (configDir: string, name: string) => Promise<void>
  runConfigExecuteCommand: (
    configDir: string,
    name: string,
    cwd: string,
  ) => Promise<RunConfigCommandResult>
  runConfigListRunning: () => Promise<RunConfigProcessSnapshot[]>
  onRunConfigPostRunResult: (
    callback: (data: { success: boolean; command: string; exitCode?: number }) => void,
  ) => () => void
}

interface RunConfiguration {
  name: string
  command: string
  args?: string
  cwd?: string
  env?: Record<string, string>
  max_instances?: number
  pre_run?: string
  post_run?: string
}

interface RunConfigFile {
  configurations: RunConfiguration[]
}

interface RunConfigSource {
  configDir: string
  relativePath: string
  file: RunConfigFile
}

type AgentType = import('../main/agents/types').AgentType
type AgentProfileMasked = import('../main/profiles/types').AgentProfileMasked
type ProfileInput = import('../main/profiles/types').ProfileInput
type ProfilePrefs = import('../main/profiles/types').ProfilePrefs

type RemoteSessionStatus = import('../main/remote/types').RemoteSessionStatus

interface RemoteTrustedDevice {
  deviceId: string
  name: string
  addedAt: string
  lastSeen: string
  publicKeyJwk: unknown
}

interface RemoteNetworkInterface {
  name: string
  address: string
  virtual: boolean
}

interface RemoteAPI {
  start: (interfaceName?: string) => Promise<{ pairingUrl: string }>
  ensureListening: (options?: { allowWithoutTrusted?: boolean }) => Promise<void>
  stop: () => Promise<void>
  getStatus: () => Promise<RemoteSessionStatus>
  acceptDevice: (remember: boolean) => Promise<void>
  rejectDevice: () => Promise<void>
  sendSignal: (msg: unknown) => Promise<void>
  listTrustedDevices: () => Promise<RemoteTrustedDevice[]>
  removeTrustedDevice: (deviceId: string) => Promise<void>
  renameTrustedDevice: (deviceId: string, name: string) => Promise<void>
  listNetworkInterfaces: () => Promise<RemoteNetworkInterface[]>
  onStatusChange: (callback: (status: RemoteSessionStatus) => void) => () => void
  onSignal: (callback: (msg: unknown) => void) => () => void
}

type TaskTrackerProvider = 'jira' | 'youtrack' | 'github'

interface TrackerConfig {
  id: string
  provider: TaskTrackerProvider
  baseUrl: string
  projectKey?: string
  /** Tracker projects belonging to this repo (whitelist for pickers/overrides); empty = all. */
  projects?: string[]
}

interface BranchTemplateConfig {
  template: string
  customVars: Record<string, string>
  typeMapping?: Record<string, string>
}

interface PRTargetRule {
  taskType: string
  targetPattern: string
}

interface PRTemplateConfig {
  titleTemplate: string
  bodyTemplate: string
  defaultTargetBranch: string
  targetRules: PRTargetRule[]
}

interface TaskFilterConfig {
  assignedToMe: boolean
  statuses: string[]
}

interface ProjectOverride {
  branchTemplate?: Partial<BranchTemplateConfig>
  prTemplate?: Partial<PRTemplateConfig>
}

interface RepoConfig {
  version: 1
  trackers: TrackerConfig[]
  branchTemplate?: BranchTemplateConfig
  prTemplate?: PRTemplateConfig
  /** Template overrides keyed by tracker project key (the task-key prefix, e.g. GAKKO). */
  projectOverrides: Record<string, ProjectOverride>
  filters: TaskFilterConfig
  /**
   * Optional CI integration (TeamCity). RAW, possibly malformed value — it round-trips
   * saves verbatim so a bad hand-edited block is never deleted from the git-tracked
   * file. Read the validated form via `window.api.ciConfig()` instead.
   */
  ci?: unknown
}

type ConfigSource = 'global' | 'repo'

interface ResolvedConfig {
  config: RepoConfig
  source: {
    branchTemplate: ConfigSource | 'default'
    prTemplate: ConfigSource | 'default'
    filters: ConfigSource | 'default'
  }
  hasGlobal: boolean
  hasRepo: boolean
  /** Trackers declared by the repo's own config — merged trackers outside this list are personal. */
  repoTrackerIds: string[]
}

interface TaskTrackerConnectionInfo {
  id: string
  provider: TaskTrackerProvider
  name: string
  baseUrl: string
  projectKey: string
  boardId?: string
  username?: string
}

interface TrackerTask {
  key: string
  summary: string
  description: string
  status: string
  statusCategory?: 'todo' | 'in-progress' | 'done'
  priority: string
  type: string
  typeName?: string
  typeIconUrl?: string
  parentKey?: string
  sprintName?: string
  sprintNumber?: number
  assignee?: string
  url?: string
}

interface TaskTrackerBranchFromTaskInput {
  connectionId: string
  task: TrackerTask
  boardId?: string
  branchType?: string
  repoRoot: string
}

interface TaskTrackerCreateBranchFromTaskInput extends TaskTrackerBranchFromTaskInput {
  baseBranch: string
  stashBeforeCreate?: boolean
}

interface TaskTrackerCreateWorktreeFromTaskInput extends TaskTrackerBranchFromTaskInput {
  worktreePath: string
  baseBranch: string
  /** User-edited branch name; when set it is sanitized and used instead of the rendered template. */
  branchName?: string
}

interface TaskTrackerBranchFromTaskResult {
  branchName: string
}

interface TaskTrackerCreateWorktreeFromTaskResult extends TaskTrackerBranchFromTaskResult {
  worktreePath: string
}

interface TaskTrackerBuildTaskContextInput {
  connectionId: string
  task: TrackerTask
  repoRoot?: string
  trackerId?: string
}

interface TrackerBoard {
  id: string
  name: string
  projectKey?: string
}

interface TrackerStatus {
  id: string
  name: string
  statusCategory?: 'todo' | 'in-progress' | 'done'
}

interface TrackerTransitionField {
  key: string
  name: string
  required: boolean
  allowedValues?: { id: string; name: string }[]
}

interface TrackerTransition {
  id: string
  name: string
  toStatus: string
  toStatusCategory?: 'todo' | 'in-progress' | 'done'
  fields: TrackerTransitionField[]
}

interface TrackerComment {
  id: string
  author: string
  body: string
  created: string
}

interface TrackerSprint {
  id: string
  name: string
  number?: number
  state: 'active' | 'closed' | 'future'
}

interface TrackerUser {
  id: string
  displayName: string
  avatarUrl?: string
}

interface CreatedTask {
  key: string
  url?: string
  /** Post-create steps that failed after the task itself was created (partial state). */
  warnings: string[]
}

/** Validated CI config of a repo, as resolved by the main process (`ci:config`). */
interface CiConfigInfo {
  provider: 'teamcity'
  baseUrl: string
  buildTypes: Array<{ id: string; label: string }>
  /** Entries dropped at parse time (typo'd ids, cap overflow) of a hand-edited
      file — the configurator announces them before a Save deletes them. */
  droppedBuildTypes?: number
  /** Capped sample of the dropped ids — named in the configurator's warning. */
  droppedBuildTypeIds?: string[]
}

/** Structured `ci:config` answer — `invalid`'s scope gates the recovery routes. */
interface CiConfigResult {
  config: CiConfigInfo | null
  /** Present when a ci block EXISTS but cannot be used (config is null then). */
  invalid?: { scope: 'file' | 'block'; message: string }
}

/** A running, queued or recently finished build in the server-wide activity view. */
interface CiActivityBuild {
  id: number
  number: string | undefined
  state: 'running' | 'queued' | 'finished'
  status: string | undefined
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
  buildTypeId: string
  buildTypeName: string
}

interface CiActivity {
  running: CiActivityBuild[]
  queued: CiActivityBuild[]
  recent: CiActivityBuild[]
}

/** A build configuration on the TeamCity server (config picker source). */
interface CiServerBuildType {
  id: string
  name: string
  projectName: string
}

/** One "Run custom build" prompt parameter (dynamic trigger form). */
interface CiParameter {
  name: string
  kind: 'text' | 'password' | 'checkbox' | 'select'
  label: string
  description: string | undefined
  required: boolean
  defaultValue: string
  options: string[] | undefined
  multiple: boolean
  valueSeparator: string
  checkedValue: string | undefined
  uncheckedValue: string | undefined
}

interface CiBuildStatus {
  id: number
  number: string
  state: 'queued' | 'running' | 'finished'
  status: 'SUCCESS' | 'FAILURE' | 'ERROR' | 'UNKNOWN'
  percentageComplete: number | undefined
  webUrl: string
  branchName: string | undefined
}

interface CiBuildTypeStatus {
  buildTypeId: string
  label: string
  build: CiBuildStatus | null
  error?: string
}

interface CiStatusResponse {
  configured: boolean
  baseUrl?: string
  hasToken?: boolean
  rows: CiBuildTypeStatus[]
  /** Set when the repo has CI configured but the fetch failed (auth missing, API error). */
  error?: string
}

interface CiTriggerResult {
  buildId: number
  webUrl: string
  branchName: string | undefined
}

type SessionStatusType =
  'idle' | 'thinking' | 'toolCalling' | 'compacting' | 'waitingPermission' | 'error' | 'ended'

interface NotchSessionStatus {
  ptySessionId: string
  windowId: number
  workspaceName: string
  branch: string | null
  status: SessionStatusType
  toolName?: string
  detail?: string
  title?: string
}

interface NotchOverlayState {
  sessions: NotchSessionStatus[]
  notchWidth: number
  notchHeight: number
  peekSessionIds?: string[]
}

interface GitHubPRInfo {
  number: number
  title: string
  state: string
  url: string
  headRefName: string
  baseRefName: string
  isDraft: boolean
  reviewDecision: string | null
  checksState: string | null
}

type GitHubBranchPRMap = Record<string, GitHubPRInfo>

interface GitHubRepoInfo {
  id: string
  defaultBranch: string
}

interface NotchAPI {
  onStateUpdate: (callback: (state: NotchOverlayState) => void) => () => void
  focusSession: (windowId: number, ptySessionId: string) => Promise<void>
  setMouseIgnore: (ignore: boolean) => void
}

declare global {
  interface Window {
    api: CanopyAPI
    notchApi: NotchAPI
  }
}
