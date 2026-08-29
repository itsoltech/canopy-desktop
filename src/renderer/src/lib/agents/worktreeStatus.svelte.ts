import { match, P } from 'ts-pattern'
import { agentSessions } from './agentState.svelte'
import type { AgentStatus } from './agentState.svelte'
import { getTabsForWorktree } from '../stores/tabs.svelte'
import { allPanes } from '../stores/splitTree'

export type AggregateAgentStatus = 'none' | 'idle' | 'working' | 'waitingPermission' | 'error'

const statusPriority: Record<AggregateAgentStatus, number> = {
  none: 0,
  idle: 1,
  working: 2,
  error: 3,
  waitingPermission: 4,
}

// Takes the status union rather than a bare string so a new AgentStatus member
// fails the build here instead of silently falling through to 'none'.
function statusTypeToAggregate(t: AgentStatus['type']): AggregateAgentStatus {
  return match(t)
    .with('waitingPermission', () => 'waitingPermission' as const)
    .with('error', () => 'error' as const)
    .with(P.union('thinking', 'toolCalling', 'compacting'), () => 'working' as const)
    .with('idle', () => 'idle' as const)
    .with(P.union('inactive', 'starting', 'ended'), () => 'none' as const)
    .exhaustive()
}

export function getWorktreeAgentStatus(worktreePath: string): AggregateAgentStatus {
  const tabs = getTabsForWorktree(worktreePath)
  const panes = tabs.flatMap((t) => allPanes(t.rootSplit))
  const agentPanes = panes.filter((p) => agentSessions[p.sessionId] !== undefined)
  if (agentPanes.length === 0) return 'none'

  let best: AggregateAgentStatus = 'none'
  for (const p of agentPanes) {
    const s = agentSessions[p.sessionId]
    if (!s) continue
    const agg = statusTypeToAggregate(s.status.type)
    if (statusPriority[agg] > statusPriority[best]) best = agg
    if (best === 'waitingPermission') return best
  }
  return best
}
