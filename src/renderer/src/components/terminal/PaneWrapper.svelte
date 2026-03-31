<script lang="ts">
  import type { PaneSession } from '../../lib/stores/splitTree'
  import { restartPane, updatePaneTitle } from '../../lib/stores/tabs.svelte'
  import { dragState, setDropTarget, type DropZone } from '../../lib/stores/dragState.svelte'
  import { claudeSessions } from '../../lib/claude/claudeState.svelte'
  import TerminalInstance from '../../lib/terminal/TerminalInstance.svelte'
  import BrowserPane from '../browser/BrowserPane.svelte'
  import EditorPane from '../editor/EditorPane.svelte'
  import ClaudeInspector from '../claude/ClaudeInspector.svelte'
  import ExitBanner from './ExitBanner.svelte'

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

  let claudeState = $derived(
    pane.toolId === 'claude' ? (claudeSessions[pane.sessionId] ?? null) : null,
  )
  let showInspector = $derived(
    pane.inspectorOpen !== false && pane.toolId === 'claude' && claudeState !== null,
  )

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
  {:else}
    <div class="pane-with-inspector">
      <div class="pane-content">
        {#key pane.sessionId}
          <TerminalInstance
            sessionId={pane.sessionId}
            wsUrl={pane.wsUrl}
            active={active && focused}
            onTitleChange={(title) => updatePaneTitle(pane.sessionId, title)}
          />
        {/key}
        {#if !pane.isRunning}
          <ExitBanner
            exitCode={pane.exitCode}
            onRestart={() => restartPane(worktreePath, tabId, pane.id)}
          />
        {/if}
      </div>
      {#if showInspector}
        <ClaudeInspector state={claudeState} />
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
    outline: 1px solid rgba(116, 192, 252, 0.4);
    outline-offset: -1px;
  }

  .pane-with-inspector {
    display: flex;
    flex-direction: row;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .pane-content {
    flex: 1;
    min-width: 0;
    position: relative;
    height: 100%;
  }

  .drop-zone-overlay {
    position: absolute;
    background: rgba(116, 192, 252, 0.15);
    border: 2px solid rgba(116, 192, 252, 0.4);
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
