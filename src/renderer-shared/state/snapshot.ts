/**
 * Serialized snapshot of the Canopy host's relevant stores, sent to the
 * remote peer as the `state.getSnapshot` RPC response. The single-window
 * scope is intentional: the snapshot mirrors just the renderer that owns
 * the current remote session. Multi-window cross-snapshot is deferred to
 * `RemoteSessionService` orchestrating all host windows (future work).
 *
 * The shape is flat on purpose — rather than nesting `tabs` inside
 * `worktrees` inside `projects`, we keep `tabsByWorktree` and
 * `activeTabByWorktree` at the top so the peer's mirror stores can patch
 * each one independently when a delta arrives (e.g. a tab was added in
 * worktree-X without touching the projects list).
 */

export interface StateSnapshot {
  /** Bumped whenever the wire format changes so stale clients can refuse. */
  protocolVersion: string
  hostInfo: HostInfoSnapshot
  projects: ProjectSnapshot[]
  tabsByWorktree: Record<string, TabSnapshot[]>
  activeTabByWorktree: Record<string, string>
  activeWorktreePath: string | null
  tools: ToolSnapshot[]
}

export interface HostInfoSnapshot {
  hostname: string
  lanIp: string
}

export interface ProjectSnapshot {
  /** Workspace DB row id. */
  id: string
  /** Absolute filesystem path to the project root. */
  path: string
  name: string
  isGitRepo: boolean
  /** Absolute path, null if the project is not a git repo. */
  repoRoot: string | null
  worktrees: WorktreeSnapshot[]
}

export interface WorktreeSnapshot {
  path: string
  branch: string
  isMain: boolean
}

export interface TabSnapshot {
  id: string
  toolId: string
  toolName: string
  /** Display name, may include a dedup counter like "Shell #2". */
  name: string
  worktreePath: string
  /**
   * The focused pane's semantic type, used by the peer UI to pick which
   * view to render (terminal vs browser vs editor vs diff). Undefined is
   * treated as "terminal" by the consumer.
   */
  paneType?: 'terminal' | 'browser' | 'editor' | 'diff'
  /**
   * Session id of the focused pane — required to send input (`pty.write` /
   * `agent.sendInput`) and to subscribe to PTY output. Undefined for panes
   * that don't back onto a PTY (e.g. browser panes).
   */
  focusedSessionId?: string
  /**
   * URL of the focused pane if it's a browser pane. Used by the peer's
   * "open externally" action to hand off the URL to the peer's own browser.
   */
  focusedUrl?: string
}

export interface ToolSnapshot {
  id: string
  name: string
  icon: string
  category: string
  isCustom: boolean
  available: boolean
}
