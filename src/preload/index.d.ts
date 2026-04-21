interface PtySpawnResult {
  sessionId: string
  wsUrl: string
}

interface ToolSpawnResult {
  sessionId: string
  wsUrl: string
  toolId: string
  toolName: string
  tmuxSessionName?: string
}

interface PtyExitData {
  sessionId: string
  exitCode: number
  signal: number
  tmuxSessionName?: string
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
  devToolsMode: 'bottom' | 'left'
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
  onCrashReport: (
    callback: (data: {
      timestamp: string
      type: string
      errorMessage: string
      stack?: string
      appVersion: string
      electronVersion: string
      os: string
    }) => void,
  ) => () => void

  // PTY
  spawnPty: (options?: { cols?: number; rows?: number; cwd?: string }) => Promise<PtySpawnResult>
  resizePty: (sessionId: string, cols: number, rows: number) => Promise<void>
  killPty: (sessionId: string, killTmux?: boolean) => Promise<void>
  writePty: (sessionId: string, data: string) => Promise<void>
  hasChildProcess: (sessionId: string) => Promise<boolean>
  getPtyDimensions: (sessionId: string) => Promise<{ cols: number; rows: number } | null>

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

  // Environment / Dependencies
  checkDependencies: (tools: string[]) => Promise<DependencyCheckResult>

  // Tools
  listTools: () => Promise<ToolDefinition[]>
  getTool: (id: string) => Promise<ToolDefinition | null>
  checkToolAvailability: () => Promise<Record<string, boolean>>
  spawnTool: (
    toolId: string,
    worktreePath: string,
    options?: {
      cols?: number
      rows?: number
      workspaceName?: string
      branch?: string
      resumeSessionId?: string
      profileId?: string
    },
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
  // App / Shell
  showInFolder: (path: string) => Promise<void>
  newWindow: () => Promise<void>
  setWorkspacePath: (path: string) => Promise<void>
  detachProject: (path: string) => Promise<void>
  focusWindowForPath: (path: string) => Promise<boolean>
  focusRendererWebContents: () => Promise<void>
  setFocusedAgentSession: (ptySessionId: string | null) => Promise<void>

  // Dialog
  openFolder: (defaultPath?: string) => Promise<string | null>

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
  gitFetch: (repoRoot: string) => Promise<void>
  gitFetchAll: (repoRoot: string) => Promise<void>
  gitStash: (repoRoot: string) => Promise<void>
  gitStashPop: (repoRoot: string) => Promise<void>
  gitBranches: (repoRoot: string) => Promise<GitBranchList>
  gitBranchCreate: (repoRoot: string, name: string, baseBranch: string) => Promise<void>
  gitCheckout: (repoRoot: string, branch: string) => Promise<void>
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
  gitWorktreeCheckout: (
    repoRoot: string,
    path: string,
    branch: string,
    createLocalTracking: boolean,
  ) => Promise<void>
  gitWorktreeRemove: (repoRoot: string, path: string, force: boolean) => Promise<void>
  gitUnmergedCommits: (repoRoot: string, branch: string) => Promise<string[]>
  gitStatusPorcelain: (repoRoot: string, worktreePath?: string) => Promise<string>
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

  // Layouts
  saveLayout: (workspaceId: string, worktreePath: string, layoutJson: string) => Promise<void>
  getLayout: (workspaceId: string, worktreePath: string) => Promise<string | null>
  getAllLayouts: (workspaceId: string) => Promise<{ worktree_path: string; layout_json: string }[]>

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
  readDir: (dirPath: string) => Promise<DirEntry[]>
  readFile: (filePath: string, maxBytes?: number) => Promise<FileReadResult>

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
  trackerConfigFetchStatuses: (
    repoRoot?: string,
    trackerId?: string,
    boardId?: string,
  ) => Promise<TrackerStatus[]>
  trackerConfigFetchTasks: (
    repoRoot?: string,
    trackerId?: string,
    params?: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
  ) => Promise<TrackerTask[]>
  trackerConfigGetCurrentUser: (repoRoot?: string, trackerId?: string) => Promise<string>
  trackerConfigFetchTaskComments: (
    repoRoot: string | undefined,
    taskKey: string,
    trackerId?: string,
  ) => Promise<TrackerComment[]>
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
  taskTrackerResolveBranchName: (
    connectionId: string,
    task: TrackerTask,
    boardId?: string,
    branchType?: string,
    repoRoot?: string,
  ) => Promise<string>
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
    boardId?: string,
  ) => Promise<{ url: string; title: string; targetBranch: string }>

  taskTrackerFindPR: (repoRoot: string, branch: string) => Promise<string | null>

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
    wsBridgeCount: number
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
  runConfigExecute: (
    configDir: string,
    name: string,
    cwd?: string,
  ) => Promise<{ sessionId: string; wsUrl: string }>
  onRunConfigPostRunResult: (
    callback: (data: { success: boolean; command: string; exitCode?: number }) => void,
  ) => () => void

  sdkAgent: {
    create: (args: {
      workspaceId: string
      worktreePath: string
      profileId: string
    }) => Promise<{ conversationId: string } | { error: string }>
    send: (args: {
      conversationId: string
      text: string
      attachments?: SdkAttachment[]
      modelOverride?: string
      permissionModeOverride?: SdkPermissionMode
    }) => Promise<{ ok: true } | { error: string }>
    cancel: (conversationId: string) => Promise<void>
    close: (conversationId: string) => Promise<void>
    delete: (conversationId: string) => Promise<void>
    list: (workspaceId: string) => Promise<SdkConversation[]>
    getTranscript: (conversationId: string) => Promise<{
      conversation: SdkConversation | undefined
      messages: SdkMessageRecord[]
    }>
    search: (args: {
      workspaceId: string
      query: string
      limit?: number
    }) => Promise<SdkConversationSearchHit[]>
    respondPermission: (args: {
      conversationId: string
      requestId: string
      decision: SdkToolDecision
    }) => Promise<void>
    respondQuestion: (args: {
      conversationId: string
      requestId: string
      answers: Record<string, SdkAskUserQuestionAnswer>
    }) => Promise<void>
    respondPlan: (args: {
      conversationId: string
      requestId: string
      decision: SdkPlanDecision
    }) => Promise<void>
    subscribe: (conversationId: string, handler: (event: SdkAgentEvent) => void) => () => void
  }
}

type SdkAgentEvent = import('../main/sdkAgents/types').SdkAgentEvent
type SdkAttachment = import('../main/sdkAgents/types').Attachment
type SdkPermissionMode = import('../main/sdkAgents/types').PermissionMode
type SdkToolDecision = import('../main/sdkAgents/types').ToolDecision
type SdkPlanDecision = import('../main/sdkAgents/types').PlanDecision
type SdkAskUserQuestionAnswer = import('../main/sdkAgents/types').AskUserQuestionAnswer
type SdkConversation = import('../main/db/sdkAgentRows').Conversation
type SdkMessageRecord = import('../main/db/sdkAgentRows').SdkMessageRecord
type SdkConversationSearchHit = import('../main/db/ConversationStore').ConversationSearchHit

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

interface RemoteAPI {
  start: () => Promise<{ pairingUrl: string }>
  ensureListening: () => Promise<void>
  stop: () => Promise<void>
  getStatus: () => Promise<RemoteSessionStatus>
  acceptDevice: (remember: boolean) => Promise<void>
  rejectDevice: () => Promise<void>
  sendSignal: (msg: unknown) => Promise<void>
  listTrustedDevices: () => Promise<RemoteTrustedDevice[]>
  removeTrustedDevice: (deviceId: string) => Promise<void>
  onStatusChange: (callback: (status: RemoteSessionStatus) => void) => () => void
  onSignal: (callback: (msg: unknown) => void) => () => void
}

type TaskTrackerProvider = 'jira' | 'youtrack' | 'github'

interface TrackerConfig {
  id: string
  provider: TaskTrackerProvider
  baseUrl: string
  projectKey?: string
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

interface BoardOverride {
  branchTemplate?: Partial<BranchTemplateConfig>
  prTemplate?: Partial<PRTemplateConfig>
}

interface RepoConfig {
  version: 1
  trackers: TrackerConfig[]
  branchTemplate?: BranchTemplateConfig
  prTemplate?: PRTemplateConfig
  boardOverrides: Record<string, BoardOverride>
  filters: TaskFilterConfig
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
  priority: string
  type: string
  parentKey?: string
  sprintName?: string
  sprintNumber?: number
  assignee?: string
  url?: string
}

interface TrackerBoard {
  id: string
  name: string
  projectKey?: string
}

interface TrackerStatus {
  id: string
  name: string
}

interface TrackerSprint {
  id: string
  name: string
  number?: number
  state: 'active' | 'closed' | 'future'
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
