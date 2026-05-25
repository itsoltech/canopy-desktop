export interface CommandWarning {
  code: 'stale-paths-removed' | 'git-watch-failed' | 'tracker-auth-required' | 'layout-ignored'
  message: string
  paths?: string[]
}

export interface WorkspaceRowSnapshot {
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

export interface GitWorktreeSnapshot {
  path: string
  head: string
  branch: string
  isMain: boolean
  isBare: boolean
}

export interface ProjectSnapshot {
  workspace: WorkspaceRowSnapshot
  isGitRepo: boolean
  repoRoot: string | null
  worktrees: GitWorktreeSnapshot[]
}

export interface WorkspaceStateSnapshot {
  project: ProjectSnapshot | null
  selectedWorktreePath: string | null
  branch: string | null
  isDirty: boolean
  aheadBehind: { ahead: number; behind: number } | null
}

export interface WorkspaceCommandResult {
  projects: ProjectSnapshot[]
  workspaceState: WorkspaceStateSnapshot
  restoredLayouts?: Array<{ worktreePath: string; layoutJson: string }>
  focusedExistingWindow?: boolean
  warnings: CommandWarning[]
}

export type PaneKind = 'terminal' | 'browser' | 'notes' | 'drawing' | 'editor' | 'diff'

export interface PaneSnapshot {
  id: string
  sessionId: string
  wsUrl: string
  toolId: string
  toolName: string
  isRunning: boolean
  exitCode: number | null
  title: string | null
  paneType?: PaneKind
  tmuxSessionName?: string
  detached?: boolean
  url?: string
  filePath?: string
  editorFiles?: Array<{ filePath: string }>
  editorActiveFile?: string
  profileId?: string
  profileName?: string
}

export type SplitSnapshot =
  | { type: 'leaf'; pane: PaneSnapshot }
  | {
      type: 'split'
      direction: 'horizontal' | 'vertical'
      ratio: number
      first: SplitSnapshot
      second: SplitSnapshot
    }

export interface TabSnapshot {
  id: string
  toolId: string
  toolName: string
  name: string
  worktreePath: string
  rootSplit: SplitSnapshot
  focusedPaneId: string
}

export interface TabCommandResult {
  worktreePath: string
  tabs: TabSnapshot[]
  activeTabId: string | null
  openedTab?: TabSnapshot
  restartedPane?: PaneSnapshot
  closedTabId?: string
}

export interface AgentCommandResult {
  sessionId: string
  tabId?: string
  paneId?: string
}

export interface RunConfigProcessSnapshot {
  sessionId: string
  name: string
  configDir: string
  worktreePath: string
}

export interface RunConfigCommandResult {
  sessionId: string
  wsUrl: string
  running: RunConfigProcessSnapshot[]
}
