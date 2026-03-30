interface PtySpawnResult {
  sessionId: string
  wsUrl: string
}

interface ToolSpawnResult {
  sessionId: string
  wsUrl: string
  toolId: string
  toolName: string
}

interface PtyExitData {
  sessionId: string
  exitCode: number
  signal: number
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

interface GitInfo {
  isGitRepo: boolean
  repoRoot: string | null
  branch: string | null
  worktrees: GitWorktreeInfo[]
  isDirty: boolean
  aheadBehind: { ahead: number; behind: number } | null
}

interface GitWorktreeInfo {
  path: string
  head: string
  branch: string
  isMain: boolean
  isBare: boolean
}

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

interface GitBranchList {
  local: string[]
  remote: string[]
  current: string | null
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
  devToolsMode: 'bottom' | 'right'
}

interface BrowserBounds {
  x: number
  y: number
  width: number
  height: number
}

interface DirEntry {
  name: string
  isDirectory: boolean
  size: number
}

type FileReadResult =
  | { content: string; truncated: boolean; size: number; binary: false }
  | { binary: true; size: number }

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
  onUpdateAvailable: (callback: (data: UpdateInfo) => void) => () => void
  onUpdateProgress: (callback: (data: UpdateProgress) => void) => () => void
  onUpdateDownloaded: (callback: (data: UpdateInfo) => void) => () => void
  onUpdateNotAvailable: (callback: () => void) => () => void
  onUpdateError: (callback: (data: { message: string }) => void) => () => void
  onUpdateInstalling: (callback: () => void) => () => void

  // Changelog
  getChangelogSinceVersion: (fromVersion: string) => Promise<ChangelogEntry[] | null>
  onShowChangelog: (callback: (data: { fromVersion: string }) => void) => () => void

  // PTY
  spawnPty: (options?: { cols?: number; rows?: number; cwd?: string }) => Promise<PtySpawnResult>
  resizePty: (sessionId: string, cols: number, rows: number) => Promise<void>
  killPty: (sessionId: string) => Promise<void>
  writePty: (sessionId: string, data: string) => Promise<void>
  hasChildProcess: (sessionId: string) => Promise<boolean>

  // Workspaces
  listWorkspaces: (limit?: number) => Promise<WorkspaceRow[]>
  getWorkspace: (id: string) => Promise<WorkspaceRow | null>
  getWorkspaceByPath: (path: string) => Promise<WorkspaceRow | null>
  upsertWorkspace: (workspace: {
    path: string
    name: string
    isGitRepo: boolean
  }) => Promise<WorkspaceRow>
  removeWorkspace: (id: string) => Promise<void>
  touchWorkspace: (id: string) => Promise<void>

  // Preferences
  getPref: (key: string) => Promise<string | null>
  setPref: (key: string, value: string) => Promise<void>
  getAllPrefs: () => Promise<Record<string, string>>
  deletePref: (key: string) => Promise<void>

  // Tools
  listTools: () => Promise<ToolDefinition[]>
  getTool: (id: string) => Promise<ToolDefinition | null>
  checkToolAvailability: () => Promise<Record<string, boolean>>
  spawnTool: (
    toolId: string,
    worktreePath: string,
    options?: { cols?: number; rows?: number; workspaceName?: string; branch?: string },
  ) => Promise<ToolSpawnResult>
  addCustomTool: (tool: {
    id: string
    name: string
    command: string
    args?: string[]
    icon?: string
    category?: string
  }) => Promise<ToolDefinition[]>
  removeCustomTool: (id: string) => Promise<ToolDefinition[]>

  // App / Shell
  showInFolder: (path: string) => Promise<void>
  newWindow: () => Promise<void>
  setWorkspacePath: (path: string) => Promise<void>
  detachProject: (path: string) => Promise<void>
  focusWindowForPath: (path: string) => Promise<boolean>
  setFocusedAgentSession: (ptySessionId: string | null) => Promise<void>

  // Dialog
  openFolder: () => Promise<string | null>

  // Workspace Git Status
  refreshWorkspaceGitStatus: (id: string, path: string) => Promise<WorkspaceRow | null>

  // Git
  gitDetect: (path: string) => Promise<GitInfo>
  gitWorktrees: (repoRoot: string) => Promise<GitWorktreeInfo[]>
  gitStatus: (path: string) => Promise<GitStatus>
  gitWatch: (repoRoot: string) => Promise<void>
  gitUnwatch: (repoRoot?: string) => Promise<void>
  gitInit: (path: string) => Promise<GitInfo>

  // Git Operations
  gitCommit: (repoRoot: string, message: string, stageAll?: boolean) => Promise<GitCommitResult>
  gitPush: (repoRoot: string) => Promise<{ branch: string; remote: string }>
  gitPull: (repoRoot: string, rebase: boolean) => Promise<{ summary: string }>
  gitFetch: (repoRoot: string) => Promise<void>
  gitFetchAll: (repoRoot: string) => Promise<void>
  gitStash: (repoRoot: string) => Promise<void>
  gitStashPop: (repoRoot: string) => Promise<void>
  gitBranches: (repoRoot: string) => Promise<GitBranchList>
  gitBranchCreate: (repoRoot: string, name: string, baseBranch: string) => Promise<void>
  gitBranchDelete: (repoRoot: string, name: string, force: boolean) => Promise<void>
  gitBranchDeleteRemote: (repoRoot: string, remote: string, name: string) => Promise<void>
  gitPushInfo: (repoRoot: string) => Promise<GitPushInfo | null>
  gitBranchMerged: (repoRoot: string, branch: string) => Promise<boolean>
  gitWorktreeAdd: (
    repoRoot: string,
    path: string,
    branch: string,
    baseBranch: string,
  ) => Promise<void>
  gitWorktreeRemove: (repoRoot: string, path: string, force: boolean) => Promise<void>
  gitUnmergedCommits: (repoRoot: string, branch: string) => Promise<string[]>
  gitStatusPorcelain: (repoRoot: string, worktreePath?: string) => Promise<string>
  gitGenerateCommitMessage: (repoRoot: string) => Promise<string | null>

  // Browser
  createBrowser: () => Promise<{ browserId: string }>
  destroyBrowser: (browserId: string) => Promise<void>
  navigateBrowser: (browserId: string, url: string) => Promise<void>
  browserBack: (browserId: string) => Promise<void>
  browserForward: (browserId: string) => Promise<void>
  browserReload: (browserId: string) => Promise<void>
  setBrowserBounds: (browserId: string, bounds: BrowserBounds) => Promise<void>
  setBrowserVisible: (browserId: string, visible: boolean) => Promise<void>
  toggleBrowserDevTools: (browserId: string, mode?: 'bottom' | 'right') => Promise<void>
  getBrowserState: (browserId: string) => Promise<BrowserState | null>
  capturePageFull: (browserId: string) => Promise<string | null>
  browserStartElementPick: (browserId: string) => Promise<string | null>
  browserStartRegionCapture: (browserId: string) => Promise<string | null>
  browserCancelPick: (browserId: string) => Promise<void>
  onBrowserUrlChanged: (callback: (data: { browserId: string; url: string }) => void) => () => void
  onBrowserTitleChanged: (
    callback: (data: { browserId: string; title: string }) => void,
  ) => () => void
  onBrowserFaviconChanged: (
    callback: (data: { browserId: string; favicon: string | null }) => void,
  ) => () => void
  onBrowserFocused: (callback: (data: { browserId: string }) => void) => () => void
  onBrowserLoadingChanged: (
    callback: (data: { browserId: string; isLoading: boolean }) => void,
  ) => () => void
  onBrowserLoadFailed: (
    callback: (data: {
      browserId: string
      errorCode: number
      errorDescription: string
      validatedURL: string
    }) => void,
  ) => () => void
  onBrowserStateChanged: (
    callback: (data: {
      browserId: string
      canGoBack: boolean
      canGoForward: boolean
      isDevToolsOpen: boolean
      devToolsMode: 'bottom' | 'right'
    }) => void,
  ) => () => void

  // Worktree Setup
  runWorktreeSetup: (
    workspaceId: string,
    repoRoot: string,
    newWorktreePath: string,
  ) => Promise<{ success: boolean; errors: string[] }>

  // Layouts
  saveLayout: (workspaceId: string, worktreePath: string, layoutJson: string) => Promise<void>
  getLayout: (workspaceId: string, worktreePath: string) => Promise<string | null>
  getAllLayouts: (workspaceId: string) => Promise<{ worktree_path: string; layout_json: string }[]>

  // Push events (main → renderer)
  onAgentHookEvent: (callback: (data: AgentHookEventData) => void) => () => void
  onAgentStatusUpdate: (callback: (data: AgentStatusData) => void) => () => void
  onAgentFocusSession: (callback: (data: { ptySessionId: string }) => void) => () => void
  onGitChanged: (callback: (info: GitInfo & { repoRoot: string }) => void) => () => void
  onPtyExit: (callback: (data: PtyExitData) => void) => () => void
  onWorktreeSetupProgress: (callback: (data: WorktreeSetupProgress) => void) => () => void
  onUrlAction: (
    callback: (data: { action: string; path: string; tool?: string; worktree?: string }) => void,
  ) => () => void

  // Menu events
  onMenuShowAbout: (callback: () => void) => () => void
  onMenuShowPreferences: (callback: () => void) => () => void

  // Filesystem
  readDir: (dirPath: string) => Promise<DirEntry[]>
  readFile: (filePath: string, maxBytes?: number) => Promise<FileReadResult>

  // File utilities
  getPathForFile: (file: File) => string
}

type SessionStatusType =
  | 'idle'
  | 'thinking'
  | 'toolCalling'
  | 'compacting'
  | 'waitingPermission'
  | 'error'
  | 'ended'

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
