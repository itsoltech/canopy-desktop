import { match } from 'ts-pattern'
import {
  DESTRUCTIVE_METHODS,
  SESSION_GRANTABLE_METHODS,
  type RpcMethodName,
} from '../../../../renderer-shared/rpc/methodList'
import { prefs } from '../stores/preferences.svelte'
import { confirm } from '../stores/dialogs.svelte'

export type ActionGuardProfile = 'none' | 'destructive' | 'full'

/**
 * Per-peer-session grants for methods in `SESSION_GRANTABLE_METHODS`. Once
 * the desktop user approves a session grant for e.g. `pty.write`, every
 * subsequent `pty.write` call from the same peer skips the confirm dialog
 * until the peer disconnects — `HostRpcServer.dispose` calls
 * `resetSessionGrants` to wipe this on disconnect.
 *
 * Lives at module scope because `RemoteHostController` is a singleton —
 * only one peer is ever connected at a time. If that ever changes (two
 * concurrent peers), move this state onto `HostRpcServer` per-instance
 * or these grants will silently cross-contaminate sessions.
 */
const sessionGrants = new Set<RpcMethodName>()

/**
 * In-flight first-time prompts keyed by method. When xterm fires several
 * `pty.write` calls before the user answers the first prompt, each
 * subsequent call finds the already-running promise here and awaits it
 * instead of opening another dialog. The renderer is single-threaded and
 * the `pendingGrants.set` call happens synchronously before any `await`,
 * so there is no check/set race.
 */
const pendingGrants = new Map<RpcMethodName, Promise<boolean>>()

/**
 * Bumped every time `resetSessionGrants` runs (i.e. whenever a peer
 * disconnects). `ensureSessionGrant` captures the current value before
 * awaiting the confirm dialog and discards the result if the generation
 * has changed by the time the user answers — otherwise a late "Allow"
 * click during a disconnect/reconnect race would leak the grant onto a
 * fresh peer session.
 */
let sessionGeneration = 0

/**
 * Decides whether a method call coming from the remote peer needs host
 * user confirmation before it runs.
 */
export async function checkAction(method: RpcMethodName, params: unknown): Promise<boolean> {
  const profile = getGuardProfile()
  return match(profile)
    .with('none', () => Promise.resolve(true))
    .with('destructive', () => {
      if (SESSION_GRANTABLE_METHODS.has(method)) return ensureSessionGrant(method)
      if (DESTRUCTIVE_METHODS.has(method)) return confirmFromDesktop(method, params)
      return Promise.resolve(true)
    })
    .with('full', () => {
      if (SESSION_GRANTABLE_METHODS.has(method)) return ensureSessionGrant(method)
      return confirmFromDesktop(method, params)
    })
    .exhaustive()
}

export function getGuardProfile(): ActionGuardProfile {
  const raw = prefs['remote.actionGuard']
  if (raw === 'none' || raw === 'destructive' || raw === 'full') return raw
  return 'destructive'
}

/**
 * Clear every session grant. Called from `HostRpcServer.dispose` whenever a
 * peer disconnects so that a reconnecting peer has to re-approve terminal
 * and agent access from scratch. Bumps `sessionGeneration` so any
 * still-pending dialog from the old session can't accidentally apply its
 * approval to the new one.
 */
export function resetSessionGrants(): void {
  sessionGeneration++
  sessionGrants.clear()
  pendingGrants.clear()
}

/**
 * Session-grant gate for `SESSION_GRANTABLE_METHODS`. Returns immediately
 * when the method is already granted; otherwise prompts once and caches
 * the result for the current peer session. Concurrent first-time calls
 * share a single in-flight dialog via `pendingGrants`. The captured
 * `generationAtCall` guards against a dispose/reconnect happening while
 * the dialog is still awaiting an answer — an "Allow" click from a
 * disconnected peer's dialog must NOT grant access to the peer that
 * replaced it.
 */
async function ensureSessionGrant(method: RpcMethodName): Promise<boolean> {
  if (sessionGrants.has(method)) return true
  const inFlight = pendingGrants.get(method)
  if (inFlight) return inFlight
  const generationAtCall = sessionGeneration
  const p = (async () => {
    try {
      const allowed = await confirmSessionGrant(method)
      if (!allowed) return false
      if (generationAtCall !== sessionGeneration) return false
      sessionGrants.add(method)
      return true
    } finally {
      if (generationAtCall === sessionGeneration) pendingGrants.delete(method)
    }
  })()
  pendingGrants.set(method, p)
  return p
}

/**
 * Variant of `confirmFromDesktop` for session-wide grants. The copy makes
 * the session-wide scope explicit so the user understands a single "Allow"
 * click opens terminal input for the whole peer connection. Same 30s
 * auto-reject race as the per-call version.
 */
async function confirmSessionGrant(method: RpcMethodName): Promise<boolean> {
  const description = describeSessionGrant(method)
  const result = await Promise.race([
    confirm({
      title: 'Remote session access request',
      message: `Remote device wants to ${description} for this session.`,
      details: `Method: ${method}. Access lasts until the remote device disconnects.`,
      confirmLabel: 'Allow for session',
      destructive: true,
    }),
    new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 30_000)),
  ])
  if (!result) {
    console.log(`[action-guard] rejected session grant "${method}"`)
  }
  return result
}

function describeSessionGrant(method: RpcMethodName): string {
  return match(method as string)
    .with('pty.write', () => 'type into any terminal')
    .with('agent.sendInput', () => 'send prompts to any agent')
    .otherwise(() => `execute ${method}`)
}

/**
 * Show a real confirm dialog to the desktop user describing the action the
 * remote peer wants to perform. Auto-rejects after 30s via a timeout race.
 */
async function confirmFromDesktop(method: RpcMethodName, params: unknown): Promise<boolean> {
  const description = describeAction(method, params)
  const result = await Promise.race([
    confirm({
      title: 'Remote action request',
      message: `Remote device wants to: ${description}`,
      details: `Method: ${method}`,
      confirmLabel: 'Allow',
      destructive: DESTRUCTIVE_METHODS.has(method),
    }),
    // Auto-reject after 30s if the desktop user ignores the prompt
    new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 30_000)),
  ])
  if (!result) {
    console.log(`[action-guard] rejected "${method}"`)
  }
  return result
}

function describeAction(method: RpcMethodName, params: unknown): string {
  const p = (typeof params === 'object' && params !== null ? params : {}) as Record<string, unknown>
  return match(method as string)
    .with('tools.spawn', () => `spawn tool "${p.toolId}" in ${p.worktreePath}`)
    .with('tabs.close', () => `close tab ${p.tabId}`)
    .with('tabs.activate', () => `activate tab ${p.tabId}`)
    .with('pty.write', () => `write to terminal ${p.sessionId}`)
    .with('pty.kill', () => `kill terminal process ${p.sessionId}`)
    .with('agent.sendInput', () => `send prompt to agent ${p.sessionId}`)
    .with('workspace.selectWorktree', () => `switch worktree to ${p.worktreePath}`)
    .with('browser.openExternal', () => `open URL externally: ${p.url}`)
    .otherwise(() => `execute ${method}`)
}
