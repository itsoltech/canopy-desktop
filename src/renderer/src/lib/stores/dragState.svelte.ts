export type DropZone = 'top' | 'bottom' | 'left' | 'right'

export interface DropTarget {
  tabId: string
  paneId: string
  zone: DropZone
}

interface DragState {
  sourceTabId: string | null
  sourceWorktree: string | null
  isDragging: boolean
  dropTarget: DropTarget | null
  aiConflict: boolean
}

export const dragState: DragState = $state({
  sourceTabId: null,
  sourceWorktree: null,
  isDragging: false,
  dropTarget: null,
  aiConflict: false,
})

export function startDrag(tabId: string, worktreePath: string): void {
  dragState.sourceTabId = tabId
  dragState.sourceWorktree = worktreePath
  dragState.isDragging = false
  dragState.dropTarget = null
}

export function activateDrag(): void {
  dragState.isDragging = true
}

export function setDropTarget(target: DropTarget | null): void {
  dragState.dropTarget = target
}

export function clearDrag(): void {
  dragState.sourceTabId = null
  dragState.sourceWorktree = null
  dragState.isDragging = false
  dragState.dropTarget = null
  dragState.aiConflict = false
}
