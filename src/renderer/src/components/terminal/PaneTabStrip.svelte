<script lang="ts">
  import { ExternalLink, X } from '@lucide/svelte'
  import type { PaneSession } from '../../lib/stores/splitTree'
  import { closePane, detachPaneToTab, movePaneToTarget } from '../../lib/stores/tabs.svelte'
  import {
    dragState,
    startPaneDrag,
    activateDrag,
    clearDrag,
  } from '../../lib/stores/dragState.svelte'

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
    onFocus()

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
      const dt = dragState.dropTarget
      if (dragState.detachToTabBar) {
        detachPaneToTab(worktreePath, tabId, pane.id)
      } else if (dt) {
        movePaneToTarget(worktreePath, tabId, pane.id, dt.tabId, dt.paneId, dt.zone)
      }
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
<div class="pane-tab-strip" class:focused onclick={onFocus} onpointerdown={handleStripPointerDown}>
  <span class="pane-label" title={label}>{label}</span>
  <div class="pane-actions">
    <button
      type="button"
      class="strip-action"
      onclick={handleDetachClick}
      title="Detach pane to tab"
      aria-label="Detach pane to tab"
    >
      <ExternalLink size={12} />
    </button>
    <button
      type="button"
      class="strip-action"
      onclick={handleCloseClick}
      title="Close pane"
      aria-label="Close pane"
    >
      <X size={12} />
    </button>
  </div>
</div>

<style>
  .pane-tab-strip {
    height: 28px;
    min-height: 28px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 6px 0 10px;
    background: var(--c-bg-glass-light);
    border-bottom: 1px solid var(--c-border-subtle);
    user-select: none;
  }

  .pane-tab-strip.focused {
    background: var(--c-active);
  }

  .pane-label {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    color: var(--c-text-secondary);
  }

  .pane-tab-strip.focused .pane-label {
    color: var(--c-text);
  }

  .pane-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  .strip-action {
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--c-text-muted);
    cursor: pointer;
    padding: 0;
  }

  .strip-action:hover {
    background: var(--c-hover-strong);
    color: var(--c-text);
  }
</style>
