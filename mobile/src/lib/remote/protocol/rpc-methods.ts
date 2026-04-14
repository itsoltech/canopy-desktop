// Copied from src/renderer-shared/rpc/methodList.ts
// Keep in sync when the wire protocol changes.

import type { StateSnapshot } from './state-snapshot'

/**
 * Explicit whitelist of RPC methods the Canopy host exposes to the remote
 * peer (phone, tablet, or another laptop). This is the single source of
 * truth for which methods the peer may call.
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

  // State mirror
  'state.getSnapshot': {
    params: void
    result: StateSnapshot
  }

  // Tools + tabs
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

  // PTY / agent input
  'pty.write': {
    params: { sessionId: string; data: string }
    result: void
  }
  /**
   * Peer-initiated resize of the host PTY. The peer computes the cols/rows
   * that fit its own viewport.
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

  // Stream subscribe
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
   * match. Returns `null` if the session is unknown or the PTY is gone.
   */
  'pty.getDimensions': {
    params: { sessionId: string }
    result: { cols: number; rows: number } | null
  }

  // Browser externally-open
  'browser.openExternal': {
    params: { url: string }
    result: { substitutedUrl: string }
  }

  // Git & worktree management — mutations on repo state.
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
}

export type RpcMethodName = keyof RpcMethods

/**
 * Argument list type for `client.call(method, ...args)`. When a method's
 * params type is `void` we drop the second positional argument.
 */
export type CallArgs<M extends RpcMethodName> = RpcMethods[M]['params'] extends void
  ? []
  : [params: RpcMethods[M]['params']]

export const DESTRUCTIVE_METHODS: ReadonlySet<RpcMethodName> = new Set<RpcMethodName>([
  'tabs.close',
  'pty.kill',
  'worktree.remove',
])

export const SESSION_GRANTABLE_METHODS: ReadonlySet<RpcMethodName> = new Set<RpcMethodName>([
  'pty.write',
  'agent.sendInput',
])
