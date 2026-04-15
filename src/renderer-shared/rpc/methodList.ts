import type { StateSnapshot } from '../state/snapshot'

/**
 * Explicit whitelist of RPC methods the Canopy host exposes to the remote
 * peer (phone, tablet, or another laptop — the protocol doesn't care).
 *
 * This is the single source of truth for which methods the peer may call —
 * the `HostRpcServer` rejects anything not listed here.
 *
 * Each entry maps a method name to its `{params, result}` shape. Method
 * names are dotted (`<namespace>.<operation>`) so the peer-side `remoteApi`
 * facade can group them semantically.
 */
export interface RpcMethods {
  // Diagnostics
  'diag.ping': {
    params: { n: number }
    result: { n: number; receivedAt: number }
  }

  // State mirror — Phase 6 delivers the initial snapshot; subsequent updates
  // arrive as delta events on the same channel.
  'state.getSnapshot': {
    params: void
    result: StateSnapshot
  }

  // Tools + tabs — Phase 7 write surface
  'tools.spawn': {
    params: { toolId: string; worktreePath: string; profileId?: string }
    result: { tabId: string }
  }
  'tabs.close': {
    params: { tabId: string }
    result: void
  }
  'tabs.activate': {
    params: { tabId: string }
    result: void
  }

  // PTY / agent input — Phase 7 (Phase 8 adds streaming output)
  'pty.write': {
    params: { sessionId: string; data: string }
    result: void
  }
  /**
   * Peer-initiated resize of the host PTY. The peer computes the cols/rows
   * that fit its own viewport (charWidth × mobile viewport width) and calls
   * this so the host PTY reshapes to match — the alternative is scrolling
   * kilometres of unused cols on a phone. When the desktop user clicks back
   * into the terminal, the host xterm's FitAddon re-fires and resizes the
   * PTY back to its own container dimensions. "Last touch wins" semantics.
   */
  'pty.resize': {
    params: { sessionId: string; cols: number; rows: number }
    result: void
  }
  'pty.kill': {
    params: { sessionId: string }
    result: void
  }
  'agent.sendInput': {
    params: { sessionId: string; text: string }
    result: void
  }

  // Workspace selection
  'workspace.selectWorktree': {
    params: { worktreePath: string }
    result: void
  }

  // Phase 8 stream subscribe (body in Phase 8)
  'pty.subscribe': {
    params: { sessionId: string }
    result: void
  }
  'pty.unsubscribe': {
    params: { sessionId: string }
    result: void
  }
  /**
   * Report the PTY's current cols/rows so the peer can size its xterm to
   * match. Without this, the peer's terminal wraps host-wide output and
   * shell/agent cursor positioning escape sequences end up in the wrong
   * columns. Returns `null` if the session is unknown or the PTY is gone.
   */
  'pty.getDimensions': {
    params: { sessionId: string }
    result: { cols: number; rows: number } | null
  }

  // Phase 9 browser externally-open
  'browser.openExternal': {
    params: { url: string }
    result: { substitutedUrl: string }
  }

  // Git & worktree management — peer-initiated mutations on repo state.
  // Each handler fans out to the matching `window.api.git*` IPC call and
  // triggers a full state rebroadcast so the peer mirror picks up the
  // new worktree list (or its removal) immediately.
  'git.listBranches': {
    params: { repoRoot: string }
    result: { local: string[]; remote: string[]; current: string | null }
  }
  'worktree.add': {
    params: { repoRoot: string; path: string; branch: string; baseBranch: string }
    result: void
  }
  'worktree.addCheckout': {
    params: {
      repoRoot: string
      path: string
      branch: string
      createLocalTracking: boolean
    }
    result: void
  }
  'worktree.remove': {
    params: { repoRoot: string; path: string; force: boolean }
    result: void
  }

  // Project management — peer can attach a new directory to the host workspace.
  'project.attach': {
    params: { path: string }
    result: void
  }
}

export type RpcMethodName = keyof RpcMethods

/**
 * Argument list type for `client.call(method, ...args)`. When a method's
 * params type is `void` we drop the second positional argument so callers
 * can write `client.call('system.getWindows')` instead of passing
 * `undefined`.
 */
export type CallArgs<M extends RpcMethodName> = RpcMethods[M]['params'] extends void
  ? []
  : [params: RpcMethods[M]['params']]

/**
 * Methods considered destructive by the action-guard system. In the
 * `'destructive'` profile these trigger a confirm modal on every call.
 * Only put methods here where a per-call prompt is tolerable (one-shot
 * operations like closing a tab or killing a PTY) — anything on a
 * keystroke path belongs in `SESSION_GRANTABLE_METHODS` instead.
 */
export const DESTRUCTIVE_METHODS: ReadonlySet<RpcMethodName> = new Set<RpcMethodName>([
  'tabs.close',
  'pty.kill',
  'worktree.remove',
])

/**
 * Methods that feed a continuous stream of peer input into the host and
 * would fire a confirm modal on every keystroke if they were in
 * `DESTRUCTIVE_METHODS`. In both the `'destructive'` and `'full'` profiles
 * the action-guard prompts the user once per peer session ("Allow terminal
 * access for this session?"), then auto-allows every subsequent call for
 * the same method until the peer disconnects. `HostRpcServer.dispose`
 * calls `resetSessionGrants` so a reconnecting peer is re-prompted.
 *
 * Still the highest-risk surface in the whitelist — both methods ultimately
 * flow peer-controlled text into a PTY where pressing Enter runs a shell
 * command. The session-grant dialog copy must make the session-wide scope
 * explicit. When adding a new method here, also add a description case in
 * `describeSessionGrant` (actionGuard.ts).
 */
export const SESSION_GRANTABLE_METHODS: ReadonlySet<RpcMethodName> = new Set<RpcMethodName>([
  'pty.write',
  'agent.sendInput',
])
