/**
 * Split a worktree's tabs into the ones that fit in the tab strip and the "…"
 * overflow menu. The active tab always stays in the strip: when it would land
 * in the overflow it takes the last visible slot, so the selected tab (its
 * status dot and close button) never disappears behind the menu.
 */
export function splitTabStrip<T extends { id: string }>(
  tabs: T[],
  visibleCount: number,
  activeId: string | undefined,
): { visible: T[]; overflow: T[] } {
  if (visibleCount <= 0 || tabs.length <= visibleCount) return { visible: tabs, overflow: [] }
  const activeIndex = tabs.findIndex((tab) => tab.id === activeId)
  if (activeIndex < visibleCount) {
    return { visible: tabs.slice(0, visibleCount), overflow: tabs.slice(visibleCount) }
  }
  return {
    visible: [...tabs.slice(0, visibleCount - 1), tabs[activeIndex]],
    overflow: [...tabs.slice(visibleCount - 1, activeIndex), ...tabs.slice(activeIndex + 1)],
  }
}
