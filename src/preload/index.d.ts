import type { ElectronAPI } from '@electron-toolkit/preload'

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

interface ClaudeHookEventData {
  ptySessionId: string
  event: {
    session_id: string
    hook_event_name: string
    tool_name?: string
    tool_input?: Record<string, unknown>
    tool_response?: string
    error?: string
    error_details?: string
    message?: string
    title?: string
    notification_type?: string
    agent_id?: string
    agent_type?: string
    reason?: string
    source?: string
    model?: string
    permission_mode?: string
    stop_hook_active?: boolean
    is_interrupt?: boolean
  }
}

interface ClaudeStatusData {
  ptySessionId: string
  status: {
    model?: { id?: string; display_name?: string }
    context_window?: {
      used_percentage?: number | null
      remaining_percentage?: number | null
      context_window_size?: number
      total_input_tokens?: number
      total_output_tokens?: number
    }
    cost?: {
      total_cost_usd?: number
      total_duration_ms?: number
      total_api_duration_ms?: number
      total_lines_added?: number
      total_lines_removed?: number
    }
    rate_limits?: {
      five_hour?: { used_percentage?: number; resets_at?: number }
      seven_day?: { used_percentage?: number; resets_at?: number }
    }
    version?: string
    session_id?: string
  }
}

interface CanopyAPI {
  // PTY
  spawnPty: (options?: { cols?: number; rows?: number; cwd?: string }) => Promise<PtySpawnResult>
  resizePty: (sessionId: string, cols: number, rows: number) => Promise<void>
  killPty: (sessionId: string) => Promise<void>
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
  onClaudeHookEvent: (callback: (data: ClaudeHookEventData) => void) => () => void
  onClaudeStatusUpdate: (callback: (data: ClaudeStatusData) => void) => () => void
  onClaudeFocusSession: (callback: (data: { ptySessionId: string }) => void) => () => void
  onGitChanged: (callback: (info: GitInfo & { repoRoot: string }) => void) => () => void
  onPtyExit: (callback: (data: PtyExitData) => void) => () => void
  onWorktreeSetupProgress: (callback: (data: WorktreeSetupProgress) => void) => () => void
  onUrlAction: (
    callback: (data: { action: string; path: string; tool?: string; worktree?: string }) => void,
  ) => () => void
}

declare global {
  interface Window {
    electron: ElectronAPI
    api: CanopyAPI
  }
}
