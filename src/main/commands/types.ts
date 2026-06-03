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

export interface WorkspaceSnapshot {
  projects: ProjectSnapshot[]
  workspaceState: WorkspaceStateSnapshot
}

export interface AppStateSnapshot {
  workspace: WorkspaceSnapshot
  tabs: TabStateSnapshot
}

export interface WorkspaceCommandResult extends WorkspaceSnapshot {
  restoredLayouts?: Array<{ worktreePath: string; layoutJson: string }>
  focusedExistingWindow?: boolean
  warnings: CommandWarning[]
}

export type PaneKind = 'terminal' | 'browser' | 'notes' | 'drawing' | 'editor' | 'diff'

export interface EditorFileSnapshot {
  filePath: string
  dirty?: boolean
  originalContent?: string
  currentContent?: string
  fileMtimeMs?: number
  fileLineEnding?: 'LF' | 'CRLF'
  externalChangeDetected?: boolean
}

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
  inspectorOpen?: boolean
  url?: string
  filePath?: string
  editorFiles?: EditorFileSnapshot[]
  editorActiveFile?: string
  profileId?: string
  profileName?: string
}

export type SplitSnapshot =
  | { type: 'leaf'; pane: PaneSnapshot }
  | {
      type: 'split'
      id: string
      direction: 'horizontal' | 'vertical'
      ratio: number
      first: SplitSnapshot
      second: SplitSnapshot
    }

export type SerializedSplitNode =
  | {
      type: 'leaf'
      toolId: string
      toolName: string
      agentSessionId?: string
      claudeSessionId?: string
      browserUrl?: string
      browserDevToolsMode?: 'bottom' | 'right'
      filePath?: string
      editorFiles?: string[]
      editorActiveFile?: string
      tmuxSessionName?: string
      profileId?: string
    }
  | {
      type: 'hsplit' | 'vsplit'
      first: SerializedSplitNode
      second: SerializedSplitNode
      ratio: number
    }

export interface SerializedLayout {
  tabs: Array<{
    toolId: string
    toolName: string
    rootSplit: SerializedSplitNode
  }>
  activeTabIndex: number
}

export interface TabSnapshot {
  id: string
  toolId: string
  toolName: string
  name: string
  worktreePath: string
  rootSplit: SplitSnapshot
  focusedPaneId: string
  suspended?: SerializedSplitNode
}

export interface TabCommandResult {
  worktreePath: string
  tabs: TabSnapshot[]
  activeTabId: string | null
  openedTab?: TabSnapshot
  restartedPane?: PaneSnapshot
  closedTabId?: string
  closedPaneId?: string
}

export type CloseWarningTarget =
  | { kind: 'tab'; tabId: string }
  | { kind: 'pane'; tabId: string; paneId: string }

export interface CloseWarningResult {
  description: string | null
}

export type TabClosePreflightResult =
  | { ok: true }
  | { ok: false; reason: 'cancelled' }
  | { ok: false; reason: 'save-failed'; failedCount: number }

export type EditorFileSaveResult =
  | { ok: true; mtimeMs: number; size: number; result: TabCommandResult }
  | { ok: false; tag: 'StaleWrite'; actualMtimeMs: number }
  | { ok: false; tag: 'WriteFailed' | 'StatFailed'; message: string }

export type EditorFileReadResult =
  | {
      ok: true
      binary: false
      content: string
      truncated: boolean
      size: number
      canWrite: boolean
      mtimeMs: number
      fileLineEnding: 'LF' | 'CRLF'
    }
  | {
      ok: true
      binary: true
      size: number
      canWrite: boolean
      mtimeMs: number
    }
  | { ok: false; tag: 'ReadFailed' | 'StatFailed'; message: string }

export type EditorFileLoadResult =
  | (Extract<EditorFileReadResult, { ok: true; binary: false }> & { result: TabCommandResult })
  | (Extract<EditorFileReadResult, { ok: true; binary: true }> & { result: TabCommandResult })
  | Extract<EditorFileReadResult, { ok: false }>

export interface TabStateSnapshot {
  tabsByWorktree: Record<string, TabSnapshot[]>
  activeTabIdByWorktree: Record<string, string | null>
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
