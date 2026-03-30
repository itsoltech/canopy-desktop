import { claudeSessions } from './claudeState.svelte'
import { getTabsForWorktree } from '../stores/tabs.svelte'
import { allPanes } from '../stores/splitTree'

export type AggregateClaudeStatus = 'none' | 'idle' | 'working' | 'waitingPermission' | 'error'

export function getWorktreeClaudeStatus(worktreePath: string): AggregateClaudeStatus {
  const tabs = getTabsForWorktree(worktreePath)
  const panes = tabs.flatMap((t) => allPanes(t.rootSplit))
  const claudePanes = panes.filter((p) => p.toolId === 'claude')
  if (claudePanes.length === 0) return 'none'

  let best: AggregateClaudeStatus = 'none'
  for (const p of claudePanes) {
    const s = claudeSessions[p.sessionId]
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
