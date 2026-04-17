import { dragState } from './dragState.svelte'
import { detachPaneToTab, movePaneToTarget } from './tabs.svelte'

export function resolvePaneDrop(worktreePath: string, tabId: string, paneId: string): void {
  const dt = dragState.dropTarget
  if (dragState.detachToTabBar) {
    detachPaneToTab(worktreePath, tabId, paneId)
  } else if (dt) {
    movePaneToTarget(worktreePath, tabId, paneId, dt.tabId, dt.paneId, dt.zone)
  }
}
