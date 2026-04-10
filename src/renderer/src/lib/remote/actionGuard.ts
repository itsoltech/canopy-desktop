import { match } from 'ts-pattern'
import { DESTRUCTIVE_METHODS, type RpcMethodName } from '../../../../renderer-shared/rpc/methodList'
import { prefs } from '../stores/preferences.svelte'
import { confirm } from '../stores/dialogs.svelte'

export type ActionGuardProfile = 'none' | 'destructive' | 'full'

/**
 * Decides whether a method call coming from the remote peer needs host
 * user confirmation before it runs.
 */
export async function checkAction(method: RpcMethodName, params: unknown): Promise<boolean> {
  const profile = getGuardProfile()
  return match(profile)
    .with('none', () => Promise.resolve(true))
    .with('destructive', () =>
      DESTRUCTIVE_METHODS.has(method) ? confirmFromDesktop(method, params) : Promise.resolve(true),
    )
    .with('full', () => confirmFromDesktop(method, params))
    .exhaustive()
}

export function getGuardProfile(): ActionGuardProfile {
  const raw = prefs['remote.actionGuard']
  if (raw === 'none' || raw === 'destructive' || raw === 'full') return raw
  return 'destructive'
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
