import type { ElectronAPI } from '@electron-toolkit/preload'

interface PtySpawnResult {
  sessionId: string
  wsUrl: string
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
}

declare global {
  interface Window {
    electron: ElectronAPI
    api: NixttyAPI
  }
}
