import { match, P } from 'ts-pattern'
import { agentSessions } from './agentState.svelte'
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

function statusTypeToAggregate(t: string): AggregateAgentStatus {
  return match(t)
    .with('waitingPermission', () => 'waitingPermission' as const)
    .with('error', () => 'error' as const)
    .with(P.union('thinking', 'toolCalling', 'compacting'), () => 'working' as const)
    .with('idle', () => 'idle' as const)
    .otherwise(() => 'none' as const)
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
