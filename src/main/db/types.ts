// --- Database row types (match SQLite columns) ---

export interface WorkspaceRow {
  id: string
  path: string
  name: string
  is_git_repo: number
  is_pinned: number
  last_opened: string | null
  cached_branch: string | null
  cached_dirty: number | null
  cached_ahead_behind: string | null
  cached_worktree_count: number | null
}

export interface ToolDefinitionRow {
  id: string
  name: string
  command: string
  args_json: string
  icon: string
  category: string
  is_custom: number
}

// --- Domain types ---

export interface Workspace {
  id: string
  path: string
  name: string
  isGitRepo: boolean
  isPinned: boolean
  lastOpened: string | null
  cachedBranch: string | null
  cachedDirty: boolean | null
  cachedAheadBehind: string | null
  cachedWorktreeCount: number | null
}

export type ToolCategory = 'ai' | 'git' | 'system' | 'shell'

export interface ToolDefinition {
  id: string
  name: string
  command: string
  args: string[]
  icon: string
  category: ToolCategory
  isCustom: boolean
}

// --- Worktree setup types ---

export interface WorktreeSetupCommandAction {
  type: 'command'
  command: string
  label?: string
}

export interface WorktreeSetupCopyAction {
  type: 'copy'
  source: string
  dest?: string
  label?: string
}

export type WorktreeSetupAction = WorktreeSetupCommandAction | WorktreeSetupCopyAction

export interface WorktreeSetupProgress {
  actionIndex: number
  totalActions: number
  label: string
  status: 'running' | 'done' | 'error'
  output?: string
  error?: string
}

// --- Runtime types (not persisted in Phase 2, used in Phase 3+) ---

export interface Worktree {
  id: string
  path: string
  branch: string
  isMain: boolean
  tabs: Tab[]
}

export interface Tab {
  id: string
  name: string
  worktreeId: string
  rootSplit: SplitNode
}

export type SplitNode =
  | { type: 'leaf'; session: TerminalSession }
  | { type: 'horizontal'; first: SplitNode; second: SplitNode; ratio: number }
  | { type: 'vertical'; first: SplitNode; second: SplitNode; ratio: number }

export interface TerminalSession {
  id: string
  tool: ToolDefinition
  worktreeId: string
  isRunning: boolean
  ptyId: string | null
  wsUrl: string | null
}

export interface ClosedTabEntry {
  tool: ToolDefinition
  worktreePath: string
  closedAt: Date
}

// --- Conversion helpers ---

export function workspaceFromRow(row: WorkspaceRow): Workspace {
  return {
    id: row.id,
    path: row.path,
    name: row.name,
    isGitRepo: row.is_git_repo === 1,
    isPinned: row.is_pinned === 1,
    lastOpened: row.last_opened,
    cachedBranch: row.cached_branch,
    cachedDirty: row.cached_dirty === null ? null : row.cached_dirty === 1,
    cachedAheadBehind: row.cached_ahead_behind,
    cachedWorktreeCount: row.cached_worktree_count,
  }
}

export function toolFromRow(row: ToolDefinitionRow): ToolDefinition {
  return {
    id: row.id,
    name: row.name,
    command: row.command,
    args: JSON.parse(row.args_json) as string[],
    icon: row.icon,
    category: row.category as ToolCategory,
    isCustom: row.is_custom === 1,
  }
}
