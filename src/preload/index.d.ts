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

interface GitStatus {
  branch: string | null
  isDirty: boolean
  aheadBehind: { ahead: number; behind: number } | null
}

interface NixttyAPI {
  // PTY
  spawnPty: (options?: { cols?: number; rows?: number; cwd?: string }) => Promise<PtySpawnResult>
  resizePty: (sessionId: string, cols: number, rows: number) => Promise<void>
  killPty: (sessionId: string) => Promise<void>

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
    options?: { cols?: number; rows?: number }
  ) => Promise<ToolSpawnResult>

  // Dialog
  openFolder: () => Promise<string | null>

  // Git
  gitDetect: (path: string) => Promise<GitInfo>
  gitWorktrees: (repoRoot: string) => Promise<GitWorktreeInfo[]>
  gitStatus: (repoRoot: string) => Promise<GitStatus>
  gitWatch: (repoRoot: string) => Promise<void>
  gitUnwatch: () => Promise<void>

  // Push events (main → renderer)
  onGitChanged: (callback: (info: GitInfo) => void) => () => void
  onPtyExit: (callback: (data: PtyExitData) => void) => () => void
}

declare global {
  interface Window {
    electron: ElectronAPI
    api: NixttyAPI
  }
}
