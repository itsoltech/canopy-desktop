import { agentSessions } from './agentState.svelte'
import { getTabsForWorktree } from '../stores/tabs.svelte'
import { allPanes } from '../stores/splitTree'

export type AggregateAgentStatus = 'none' | 'idle' | 'working' | 'waitingPermission' | 'error'

export function getWorktreeAgentStatus(worktreePath: string): AggregateAgentStatus {
  const tabs = getTabsForWorktree(worktreePath)
  const panes = tabs.flatMap((t) => allPanes(t.rootSplit))
  const agentPanes = panes.filter((p) => agentSessions[p.sessionId] !== undefined)
  if (agentPanes.length === 0) return 'none'

  let best: AggregateAgentStatus = 'none'
  for (const p of agentPanes) {
    const s = agentSessions[p.sessionId]
    if (!s) continue
    const t = s.status.type
    if (t === 'waitingPermission') return 'waitingPermission'
    if (t === 'error') best = 'error'
    else if ((t === 'thinking' || t === 'toolCalling' || t === 'compacting') && best !== 'error')
      best = 'working'
    else if (t === 'idle' && best === 'none') best = 'idle'
  }
  return best
}
