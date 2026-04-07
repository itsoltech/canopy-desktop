<script lang="ts">
  import type { PaneSession } from '../../lib/stores/splitTree'
  import { restartPane, updatePaneTitle, isAiToolId } from '../../lib/stores/tabs.svelte'
  import { dragState, setDropTarget, type DropZone } from '../../lib/stores/dragState.svelte'
  import TerminalInstance from '../../lib/terminal/TerminalInstance.svelte'
  import BrowserPane from '../browser/BrowserPane.svelte'
  import EditorPane from '../editor/EditorPane.svelte'
  import DiffPane from '../diff/DiffPane.svelte'
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
    onFocus,
  }: {
    pane: PaneSession
    tabId: string
    worktreePath: string
    focused: boolean
    active: boolean
    onFocus: () => void
  } = $props()

  let wrapperEl: HTMLDivElement | undefined = $state()
  let hoveredZone: DropZone | null = $state(null)

  let wpmEnabled = $derived(prefs['wpm.enabled'] === 'true')
  let keystrokeVisualizerEnabled = $derived(prefs['keystrokeVisualizer.enabled'] === 'true')

  // Whether this pane is a valid drop target
  let isValidTarget = $derived(
    dragState.isDragging &&
      dragState.sourceTabId !== tabId &&
      dragState.sourceWorktree === worktreePath,
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
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="pane-wrapper" class:focused onclick={onFocus} bind:this={wrapperEl}>
  {#if pane.paneType === 'browser'}
    <BrowserPane
      browserId={pane.sessionId}
      {active}
      {focused}
      initialUrl={pane.url}
      onTitleChange={(title) => updatePaneTitle(pane.sessionId, title)}
      {onFocus}
    />
  {:else if pane.paneType === 'editor'}
    <EditorPane filePath={pane.filePath!} {active} />
  {:else if pane.paneType === 'diff'}
    <DiffPane {worktreePath} {active} />
  {:else}
    <div class="pane-content">
      {#key pane.sessionId}
        <TerminalInstance
          sessionId={pane.sessionId}
          wsUrl={pane.wsUrl}
          active={active && focused}
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

  {#if hoveredZone}
    <div class="drop-zone-overlay {hoveredZone}"></div>
  {/if}
</div>

<style>
  .pane-wrapper {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .pane-wrapper.focused {
    outline: 1px solid var(--c-border);
    outline-offset: -1px;
  }

  .pane-content {
    width: 100%;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .drop-zone-overlay {
    position: absolute;
    background: var(--c-accent-bg);
    border: 2px solid var(--c-focus-ring);
    border-radius: 4px;
    pointer-events: none;
    z-index: 10;
    transition: all 0.1s ease;
  }

  .drop-zone-overlay.left {
    left: 0;
    top: 0;
    width: 50%;
    height: 100%;
  }

  .drop-zone-overlay.right {
    left: 50%;
    top: 0;
    width: 50%;
    height: 100%;
  }

  .drop-zone-overlay.top {
    left: 0;
    top: 0;
    width: 100%;
    height: 50%;
  }

  .drop-zone-overlay.bottom {
    left: 0;
    top: 50%;
    width: 100%;
    height: 50%;
  }
</style>
