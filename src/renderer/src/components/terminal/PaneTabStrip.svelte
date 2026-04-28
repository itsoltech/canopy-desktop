<script lang="ts">
  import { ExternalLink, X } from '@lucide/svelte'
  import type { PaneSession } from '../../lib/stores/splitTree'
  import { closePane, detachPaneToTab } from '../../lib/stores/tabs.svelte'
  import { startPaneDrag, activateDrag, clearDrag } from '../../lib/stores/dragState.svelte'
  import { resolvePaneDrop } from '../../lib/stores/paneDrag'

  let {
    pane,
    tabId,
    worktreePath,
    focused,
    onFocus,
  }: {
    pane: PaneSession
    tabId: string
    worktreePath: string
    focused: boolean
    onFocus: () => void
  } = $props()

  const DRAG_THRESHOLD = 5

  let dragStartX = 0
  let dragStartY = 0
  let dragActive = false

  let label = $derived(pane.title?.trim() || pane.toolName)

  function handleStripPointerDown(e: PointerEvent): void {
    if (e.button !== 0 || e.altKey) return
    const target = e.target as HTMLElement
    if (target.closest('.strip-action')) return

    e.preventDefault()

    dragStartX = e.clientX
    dragStartY = e.clientY
    dragActive = false
    startPaneDrag(tabId, pane.id, worktreePath)

    window.addEventListener('pointermove', handleDragMove)
    window.addEventListener('pointerup', handleDragEnd)
  }

  function handleDragMove(e: PointerEvent): void {
    const dx = e.clientX - dragStartX
    const dy = e.clientY - dragStartY
    if (!dragActive && Math.sqrt(dx * dx + dy * dy) > DRAG_THRESHOLD) {
      dragActive = true
      activateDrag()
    }
  }

  function handleDragEnd(): void {
    window.removeEventListener('pointermove', handleDragMove)
    window.removeEventListener('pointerup', handleDragEnd)

    if (dragActive) {
      resolvePaneDrop(worktreePath, tabId, pane.id)
    }

    dragActive = false
    clearDrag()
  }

  function handleDetachClick(e: MouseEvent): void {
    e.stopPropagation()
    detachPaneToTab(worktreePath, tabId, pane.id)
  }

  function handleCloseClick(e: MouseEvent): void {
    e.stopPropagation()
    closePane(worktreePath, tabId, pane.id)
  }

  $effect(() => {
    return () => {
      window.removeEventListener('pointermove', handleDragMove)
      window.removeEventListener('pointerup', handleDragEnd)
    }
  })
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="h-7 min-h-7 flex items-center justify-between gap-2 pl-2.5 pr-1.5 bg-bg-glass-light border-b border-border-subtle select-none"
  class:bg-active={focused}
  onclick={onFocus}
  onpointerdown={handleStripPointerDown}
>
  <span
    class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-xs"
    class:text-text={focused}
    class:text-text-secondary={!focused}
    title={label}>{label}</span
  >
  <div class="flex items-center gap-0.5 flex-shrink-0">
    <button
      type="button"
      class="strip-action w-4.5 h-4.5 inline-flex items-center justify-center border-0 rounded-sm bg-transparent text-text-muted cursor-pointer p-0 hover:bg-hover-strong hover:text-text"
      onclick={handleDetachClick}
      title="Detach pane to tab"
      aria-label="Detach pane to tab"
    >
      <ExternalLink size={12} />
    </button>
    <button
      type="button"
      class="strip-action w-4.5 h-4.5 inline-flex items-center justify-center border-0 rounded-sm bg-transparent text-text-muted cursor-pointer p-0 hover:bg-hover-strong hover:text-text"
      onclick={handleCloseClick}
      title="Close pane"
      aria-label="Close pane"
    >
      <X size={12} />
    </button>
  </div>
</div>
