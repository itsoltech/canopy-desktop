export type DropZone = 'top' | 'bottom' | 'left' | 'right'

export interface DropTarget {
  tabId: string
  paneId: string
  zone: DropZone
}

interface DragState {
  dragType: 'tab' | 'pane' | null
  sourceTabId: string | null
  sourcePaneId: string | null
  sourceWorktree: string | null
  isDragging: boolean
  dropTarget: DropTarget | null
  detachToTabBar: boolean
}

export const dragState: DragState = $state({
  dragType: null,
  sourceTabId: null,
  sourcePaneId: null,
  sourceWorktree: null,
  isDragging: false,
  dropTarget: null,
  detachToTabBar: false,
})

export function startDrag(tabId: string, worktreePath: string): void {
  dragState.dragType = 'tab'
  dragState.sourceTabId = tabId
  dragState.sourcePaneId = null
  dragState.sourceWorktree = worktreePath
  dragState.isDragging = false
  dragState.dropTarget = null
  dragState.detachToTabBar = false
}

export function startPaneDrag(tabId: string, paneId: string, worktreePath: string): void {
  dragState.dragType = 'pane'
  dragState.sourceTabId = tabId
  dragState.sourcePaneId = paneId
  dragState.sourceWorktree = worktreePath
  dragState.isDragging = false
  dragState.dropTarget = null
  dragState.detachToTabBar = false
}

export function activateDrag(): void {
  dragState.isDragging = true
}

export function setDropTarget(target: DropTarget | null): void {
  dragState.dropTarget = target
}

export function setDetachTarget(value: boolean): void {
  dragState.detachToTabBar = value
}

export function clearDrag(): void {
  dragState.dragType = null
  dragState.sourceTabId = null
  dragState.sourcePaneId = null
  dragState.sourceWorktree = null
  dragState.isDragging = false
  dragState.dropTarget = null
  dragState.detachToTabBar = false
}
