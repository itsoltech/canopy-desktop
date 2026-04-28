<script lang="ts">
  import type { PaneSession } from '../../lib/stores/splitTree'
  import { restartPane, updatePaneTitle, isAiToolId } from '../../lib/stores/tabs.svelte'
  import {
    dragState,
    setDropTarget,
    startPaneDrag,
    activateDrag,
    clearDrag,
    type DropZone,
  } from '../../lib/stores/dragState.svelte'
  import { resolvePaneDrop } from '../../lib/stores/paneDrag'
  import TerminalInstance from '../../lib/terminal/TerminalInstance.svelte'
  import BrowserPane from '../browser/BrowserPane.svelte'
  import EditorPane from '../editor/EditorPane.svelte'
  import DiffPane from '../diff/DiffPane.svelte'
  import NotesPane from '../notes/NotesPane.svelte'
  import DrawingPane from '../drawing/DrawingPane.svelte'
  import PaneTabStrip from './PaneTabStrip.svelte'
  import ExitBanner from './ExitBanner.svelte'
  import DetachedOverlay from './DetachedOverlay.svelte'
  import WpmIndicator from './WpmIndicator.svelte'
  import KeystrokeVisualizer from './KeystrokeVisualizer.svelte'
  import { prefs } from '../../lib/stores/preferences.svelte'
  import { reattachTmuxPane, killTmuxPane } from '../../lib/stores/tabs.svelte'

  let {
    pane,
    tabId,
    worktreePath,
    focused,
    active,
    isMultiPane = false,
    onFocus,
  }: {
    pane: PaneSession
    tabId: string
    worktreePath: string
    focused: boolean
    active: boolean
    isMultiPane?: boolean
    onFocus: () => void
  } = $props()

  let wrapperEl: HTMLDivElement | undefined = $state()
  let hoveredZone: DropZone | null = $state(null)

  let wpmEnabled = $derived(prefs['wpm.enabled'] === 'true')
  let keystrokeVisualizerEnabled = $derived(prefs['keystrokeVisualizer.enabled'] === 'true')

  // Whether this pane is a valid drop target (for both tab and pane drags)
  let isValidTarget = $derived(
    dragState.isDragging &&
      dragState.sourceWorktree === worktreePath &&
      (dragState.dragType === 'tab'
        ? dragState.sourceTabId !== tabId
        : dragState.sourcePaneId !== pane.id),
  )

  // Whether this pane is the drag source (for visual dimming)
  let isDragSource = $derived(
    dragState.dragType === 'pane' && dragState.sourcePaneId === pane.id && dragState.isDragging,
  )

  function computeZone(e: PointerEvent): DropZone | null {
    if (!wrapperEl) return null
    const rect = wrapperEl.getBoundingClientRect()
    if (
      e.clientX < rect.left ||
      e.clientX > rect.right ||
      e.clientY < rect.top ||
      e.clientY > rect.bottom
    ) {
      return null
    }
    const rx = (e.clientX - rect.left) / rect.width
    const ry = (e.clientY - rect.top) / rect.height
    const sum = rx + ry
    const diff = rx - ry
    if (sum < 1 && diff < 0) return 'left'
    if (sum > 1 && diff > 0) return 'right'
    if (sum < 1 && diff > 0) return 'top'
    return 'bottom'
  }

  function handlePointerMove(e: PointerEvent): void {
    const zone = computeZone(e)
    if (zone) {
      hoveredZone = zone
      setDropTarget({ tabId, paneId: pane.id, zone })
    } else if (hoveredZone !== null) {
      hoveredZone = null
      if (dragState.dropTarget?.tabId === tabId && dragState.dropTarget?.paneId === pane.id) {
        setDropTarget(null)
      }
    }
  }

  // Drop target listener (active when a drag is in progress and this pane is a valid target)
  $effect(() => {
    if (!isValidTarget) {
      hoveredZone = null
      return
    }
    window.addEventListener('pointermove', handlePointerMove)
    return () => {
      window.removeEventListener('pointermove', handlePointerMove)
      hoveredZone = null
    }
  })

  // --- Alt+drag pane initiation (capture phase to intercept before terminal) ---

  let paneDragStartX = 0
  let paneDragStartY = 0
  let paneDragActive = false

  function handlePaneDragPointerDown(e: PointerEvent): void {
    if (!e.altKey || e.button !== 0 || !isMultiPane) return
    e.preventDefault()
    e.stopPropagation()

    paneDragStartX = e.clientX
    paneDragStartY = e.clientY
    paneDragActive = false
    startPaneDrag(tabId, pane.id, worktreePath)

    window.addEventListener('pointermove', handlePaneDragMove)
    window.addEventListener('pointerup', handlePaneDragEnd)
  }

  function handlePaneDragMove(e: PointerEvent): void {
    const dx = e.clientX - paneDragStartX
    const dy = e.clientY - paneDragStartY
    if (!paneDragActive && Math.sqrt(dx * dx + dy * dy) > 5) {
      paneDragActive = true
      activateDrag()
    }
  }

  function handlePaneDragEnd(): void {
    window.removeEventListener('pointermove', handlePaneDragMove)
    window.removeEventListener('pointerup', handlePaneDragEnd)

    if (paneDragActive) {
      resolvePaneDrop(worktreePath, tabId, pane.id)
    }

    paneDragActive = false
    clearDrag()
  }

  // Attach capture-phase listener to intercept Alt+click before terminal content
  $effect(() => {
    if (!wrapperEl) return
    wrapperEl.addEventListener('pointerdown', handlePaneDragPointerDown, { capture: true })
    return () => {
      wrapperEl!.removeEventListener('pointerdown', handlePaneDragPointerDown, { capture: true })
      // Safety: clean up window listeners if component unmounts mid-drag
      window.removeEventListener('pointermove', handlePaneDragMove)
      window.removeEventListener('pointerup', handlePaneDragEnd)
    }
  })

  function dropZonePosition(zone: DropZone): string {
    return {
      left: 'left-0 top-0 w-1/2 h-full',
      right: 'left-1/2 top-0 w-1/2 h-full',
      top: 'left-0 top-0 w-full h-1/2',
      bottom: 'left-0 top-1/2 w-full h-1/2',
    }[zone]
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="relative flex flex-col w-full h-full overflow-hidden"
  class:opacity-40={isDragSource}
  onclick={onFocus}
  bind:this={wrapperEl}
>
  {#if isMultiPane}
    <PaneTabStrip {pane} {tabId} {worktreePath} {focused} {onFocus} />
  {/if}

  <div class="relative flex-1 min-h-0 overflow-hidden">
    {#if pane.paneType === 'browser'}
      <BrowserPane
        browserId={pane.sessionId}
        {worktreePath}
        {active}
        {focused}
        initialUrl={pane.url}
        onTitleChange={(title) => updatePaneTitle(pane.sessionId, title)}
        {onFocus}
      />
    {:else if pane.paneType === 'editor'}
      <EditorPane paneId={pane.id} {active} />
    {:else if pane.paneType === 'diff'}
      <DiffPane {worktreePath} {active} />
    {:else if pane.paneType === 'notes'}
      <NotesPane paneSessionId={pane.sessionId} />
    {:else if pane.paneType === 'drawing'}
      <DrawingPane />
    {:else}
      <div class="w-full h-full relative overflow-hidden">
        {#key pane.sessionId}
          <TerminalInstance
            sessionId={pane.sessionId}
            wsUrl={pane.wsUrl}
            {active}
            focused={active && focused}
            visible={active}
            isAiTool={isAiToolId(pane.toolId)}
            onTitleChange={(title) => updatePaneTitle(pane.sessionId, title)}
          />
        {/key}
        {#if wpmEnabled}
          <WpmIndicator sessionId={pane.sessionId} />
        {/if}
        {#if keystrokeVisualizerEnabled}
          <KeystrokeVisualizer sessionId={pane.sessionId} />
        {/if}
        {#if !pane.isRunning && pane.detached && pane.tmuxSessionName}
          <DetachedOverlay
            tmuxSessionName={pane.tmuxSessionName}
            onReattach={() => reattachTmuxPane(worktreePath, tabId, pane.id)}
            onKill={() => killTmuxPane(worktreePath, tabId, pane.id)}
          />
        {:else if !pane.isRunning}
          <ExitBanner
            exitCode={pane.exitCode}
            onRestart={() => restartPane(worktreePath, tabId, pane.id)}
          />
        {/if}
      </div>
    {/if}
  </div>

  {#if hoveredZone}
    <div
      class="absolute bg-accent-bg border-2 border-focus-ring rounded-md pointer-events-none z-pane-overlay transition-all duration-fast {dropZonePosition(
        hoveredZone,
      )}"
    ></div>
  {/if}
</div>
