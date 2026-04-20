<script lang="ts">
  import { onMount } from 'svelte'
  import {
    tabsByWorktree,
    activeTabId,
    switchTab,
    closeTab,
    moveTab,
    moveTabToSplit,
    getTabDisplayName,
    type TabInfo,
  } from '../../lib/stores/tabs.svelte'
  import { allPanes, findLeaf } from '../../lib/stores/splitTree'
  import { agentBadges, agentSessions, type BadgeType } from '../../lib/agents/agentState.svelte'
  import { browserSessions } from '../../lib/browser/browserState.svelte'
  import {
    dragState,
    startDrag,
    activateDrag,
    clearDrag,
    setDetachTarget,
  } from '../../lib/stores/dragState.svelte'
  import {
    connectionStatus,
    type ConnectionStatus,
  } from '../../lib/terminal/connectionState.svelte'
  onMount(() => {
    async function pollShellBusy(): Promise<void> {
      const shellPanes = tabs
        .flatMap((tab) => allPanes(tab.rootSplit))
        .filter((p) => !agentSessions[p.sessionId] && p.isRunning)

      const results = await Promise.all(
        shellPanes.map(async (p) => {
          try {
            return [p.sessionId, await window.api.hasChildProcess(p.sessionId)] as const
          } catch {
            return [p.sessionId, false] as const
          }
        }),
      )
      shellBusyState = Object.fromEntries(results)
    }

    void pollShellBusy()
    const shellPollTimer = setInterval(() => void pollShellBusy(), 2000)

    return () => {
      clearInterval(shellPollTimer)
      window.removeEventListener('pointermove', handleDragMove)
      window.removeEventListener('pointerup', handleDragEnd)
    }
  })

  function getTabFavicon(tab: TabInfo): string | null {
    const focused = findLeaf(tab.rootSplit, tab.focusedPaneId)
    if (!focused || focused.paneType !== 'browser') return null
    return browserSessions[focused.sessionId]?.favicon ?? null
  }

  function getTabBadge(tab: TabInfo): BadgeType {
    // Show badge if ANY agent pane in this tab has a notification
    const panes = allPanes(tab.rootSplit)
    for (const p of panes) {
      if (!agentSessions[p.sessionId]) continue
      const b = agentBadges[p.sessionId]
      if (b === 'permission') return 'permission'
      if (b === 'unread') return 'unread'
    }
    return 'none'
  }

  function getTabStatusDot(tab: TabInfo): { color: string; pulse: boolean; label: string } | null {
    const badge = getTabBadge(tab)
    if (badge === 'permission')
      return { color: 'var(--c-warning)', pulse: true, label: 'Permission required' }
    if (badge === 'unread')
      return { color: 'var(--c-accent)', pulse: true, label: 'Unread activity' }

    const panes = allPanes(tab.rootSplit)

    // If every pane has exited, surface that instead of falling through to an
    // "idle" state — keeps the dot in sync with the `.tab.exited` text color.
    if (panes.length > 0 && panes.every((p) => !p.isRunning)) {
      return { color: 'var(--c-blazing)', pulse: false, label: 'Exited' }
    }

    let priority = 0

    for (const p of panes) {
      if (!p.isRunning) continue
      const session = agentSessions[p.sessionId]
      if (session) {
        const t = session.status.type
        if (t === 'waitingPermission')
          return { color: 'var(--c-warning)', pulse: true, label: 'Permission required' }
        if (t === 'error') priority = Math.max(priority, 5)
        else if (t === 'thinking' || t === 'toolCalling' || t === 'compacting' || t === 'starting')
          priority = Math.max(priority, 4)
        else if (t === 'idle' || t === 'ended') priority = Math.max(priority, 3)
      } else {
        priority = Math.max(priority, shellBusyState[p.sessionId] ? 2 : 1)
      }
    }

    if (priority === 5) return { color: 'var(--c-danger)', pulse: false, label: 'Agent error' }
    if (priority === 4) return { color: 'var(--c-accent)', pulse: true, label: 'Agent working' }
    if (priority === 3) return { color: 'var(--c-success)', pulse: false, label: 'Agent idle' }
    if (priority === 2) return { color: 'var(--c-accent)', pulse: true, label: 'Shell running' }
    if (priority === 1) return { color: 'var(--c-success)', pulse: false, label: 'Shell idle' }
    return null
  }

  function getConnectionState(tab: TabInfo): ConnectionStatus | null {
    const panes = allPanes(tab.rootSplit)
    for (const p of panes) {
      const s = connectionStatus[p.sessionId]
      if (s === 'disconnected') return 'disconnected'
      if (s === 'reconnecting') return 'reconnecting'
    }
    return null
  }

  let { worktreePath }: { worktreePath: string } = $props()

  let tabs = $derived(tabsByWorktree[worktreePath] ?? [])

  let shellBusyState: Record<string, boolean> = $state({})
  let currentActiveId = $derived(activeTabId[worktreePath])

  let showOverflow = $state(false)
  let visibleCount = $state(0)
  let containerEl: HTMLDivElement | undefined = $state()

  $effect(() => {
    if (!containerEl) return undefined
    const observer = new ResizeObserver(() => {
      const containerWidth = containerEl!.clientWidth
      const minTabWidth = 80
      const maxVisible = Math.max(1, Math.floor(containerWidth / minTabWidth))
      visibleCount = maxVisible
    })
    observer.observe(containerEl)
    return () => observer.disconnect()
  })

  let visibleTabs = $derived(
    visibleCount > 0 && tabs.length > visibleCount ? tabs.slice(0, visibleCount) : tabs,
  )
  let overflowTabs = $derived(
    visibleCount > 0 && tabs.length > visibleCount ? tabs.slice(visibleCount) : [],
  )

  function handleMiddleClick(e: MouseEvent, tabId: string): void {
    if (e.button === 1) {
      e.preventDefault()
      closeTab(tabId)
    }
  }

  // --- Tab drag reordering ---

  let dragTabId: string | null = $state(null)
  let dragActive = $state(false)
  let dropTargetId: string | null = $state(null)
  let dragStartX = 0
  let suppressClick = false

  function handleTabPointerDown(e: PointerEvent, tabId: string): void {
    if (e.button !== 0) return
    dragTabId = tabId
    dragStartX = e.clientX
    dragActive = false
    dropTargetId = null
    // Only enable panel-drop mode when there are 2+ tabs
    if (tabs.length > 1) {
      startDrag(tabId, worktreePath)
    }
    window.addEventListener('pointermove', handleDragMove)
    window.addEventListener('pointerup', handleDragEnd)
  }

  function handleDragMove(e: PointerEvent): void {
    if (!dragTabId) return
    if (!dragActive && Math.abs(e.clientX - dragStartX) > 5) {
      dragActive = true
      if (tabs.length > 1) {
        activateDrag()
      }
    }
    if (!dragActive) return

    const tabEls = containerEl?.querySelectorAll<HTMLElement>('[data-tab-id]')
    if (!tabEls) {
      dropTargetId = null
      return
    }

    let found: string | null = null
    for (const el of tabEls) {
      const rect = el.getBoundingClientRect()
      if (e.clientX >= rect.left && e.clientX <= rect.right) {
        const id = el.dataset.tabId
        if (id && id !== dragTabId) found = id
        break
      }
    }
    dropTargetId = found
  }

  function handleDragEnd(): void {
    window.removeEventListener('pointermove', handleDragMove)
    window.removeEventListener('pointerup', handleDragEnd)

    // Check for panel-split drop first (drag from tab bar to a panel)
    const dt = dragState.dropTarget
    if (dragActive && dragTabId && dt) {
      void moveTabToSplit(worktreePath, dragTabId, dt.tabId, dt.paneId, dt.zone)
    } else if (dragActive && dragTabId && dropTargetId) {
      // Existing tab-reorder logic
      const fromIdx = tabs.findIndex((t) => t.id === dragTabId)
      const toIdx = tabs.findIndex((t) => t.id === dropTargetId)
      if (fromIdx >= 0 && toIdx >= 0) {
        moveTab(worktreePath, fromIdx, toIdx)
      }
    }

    const wasDragActive = dragActive

    if (wasDragActive) {
      suppressClick = true
      requestAnimationFrame(() => {
        suppressClick = false
      })
    }

    dragTabId = null
    dragActive = false
    dropTargetId = null
    clearDrag()
  }

  // --- Pane drag: tab bar as detach target + tab-switch-on-hover ---

  let isPaneDragActive = $derived(
    dragState.dragType === 'pane' &&
      dragState.isDragging &&
      dragState.sourceWorktree === worktreePath,
  )

  let paneDragHoverTabId: string | null = $state(null)
  let paneDragHoverTimer: ReturnType<typeof setTimeout> | null = null

  function handlePaneDragOverTabBar(e: PointerEvent): void {
    if (!containerEl) return

    const barRect = containerEl.getBoundingClientRect()
    const inBar =
      e.clientX >= barRect.left &&
      e.clientX <= barRect.right &&
      e.clientY >= barRect.top &&
      e.clientY <= barRect.bottom

    if (!inBar) {
      setDetachTarget(false)
      clearHoverTimer()
      paneDragHoverTabId = null
      return
    }

    // Check if pointer is over a specific tab
    const tabEls = containerEl.querySelectorAll<HTMLElement>('[data-tab-id]')
    let hoveredTab: string | null = null
    for (const el of tabEls) {
      const rect = el.getBoundingClientRect()
      if (e.clientX >= rect.left && e.clientX <= rect.right) {
        hoveredTab = el.dataset.tabId ?? null
        break
      }
    }

    if (hoveredTab) {
      // Over a tab — not a detach target, but potentially a tab-switch target
      setDetachTarget(false)

      if (hoveredTab !== dragState.sourceTabId && hoveredTab !== paneDragHoverTabId) {
        clearHoverTimer()
        paneDragHoverTabId = hoveredTab
        paneDragHoverTimer = setTimeout(() => {
          if (paneDragHoverTabId) {
            void switchTab(paneDragHoverTabId)
          }
        }, 300)
      }
    } else {
      // Over the tab bar but not over any tab — detach target
      setDetachTarget(true)
      clearHoverTimer()
      paneDragHoverTabId = null
    }
  }

  function clearHoverTimer(): void {
    if (paneDragHoverTimer) {
      clearTimeout(paneDragHoverTimer)
      paneDragHoverTimer = null
    }
  }

  $effect(() => {
    if (!isPaneDragActive) {
      clearHoverTimer()
      paneDragHoverTabId = null
      return
    }
    window.addEventListener('pointermove', handlePaneDragOverTabBar)
    return () => {
      window.removeEventListener('pointermove', handlePaneDragOverTabBar)
      clearHoverTimer()
      paneDragHoverTabId = null
    }
  })
</script>

{#if tabs.length > 0}
  <div class="tab-bar" class:drag-active={dragActive} bind:this={containerEl}>
    <div class="tabs-row">
      {#each visibleTabs as tab (tab.id)}
        {@const connState = getConnectionState(tab)}
        {@const favicon = getTabFavicon(tab)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="tab"
          class:active={tab.id === currentActiveId}
          class:exited={!tab.suspended && allPanes(tab.rootSplit).some((p) => !p.isRunning)}
          class:dragging={dragActive && dragTabId === tab.id}
          class:drop-target={dragActive && dropTargetId === tab.id}
          data-tab-id={tab.id}
          onclick={async () => {
            if (!suppressClick) await switchTab(tab.id)
          }}
          onauxclick={(e) => handleMiddleClick(e, tab.id)}
          onpointerdown={(e) => handleTabPointerDown(e, tab.id)}
          title={getTabDisplayName(tab)}
        >
          {#if favicon}
            <img class="tab-favicon" src={favicon} alt="" width="12" height="12" />
          {:else}
            {@const dot = getTabStatusDot(tab)}
            <span
              class="tab-status-dot"
              class:pulse={dot?.pulse ?? false}
              style:background={dot?.color ?? 'var(--c-text-faint)'}
              role={dot ? 'status' : undefined}
              aria-label={dot?.label ?? undefined}
              title={dot?.label ?? undefined}
            ></span>
          {/if}
          <span class="tab-name">{getTabDisplayName(tab)}</span>
          {#if connState}
            <span
              class="tab-badge connection-badge"
              class:disconnected={connState === 'disconnected'}
              role="status"
              aria-label={connState === 'disconnected' ? 'Disconnected' : 'Reconnecting'}
              title={connState === 'disconnected' ? 'Disconnected' : 'Reconnecting...'}
            ></span>
          {/if}
          <button
            class="tab-close"
            onclick={(e: MouseEvent) => {
              e.stopPropagation()
              closeTab(tab.id)
            }}
            title="Close tab"
            aria-label="Close tab"
          >
            &times;
          </button>
        </div>
      {/each}
    </div>

    {#if isPaneDragActive && dragState.detachToTabBar}
      <div class="detach-indicator" aria-label="Drop to detach pane as new tab">+</div>
    {/if}

    {#if overflowTabs.length > 0}
      <div class="overflow-wrapper">
        <button
          class="overflow-trigger"
          aria-label="Show more tabs"
          onclick={() => {
            showOverflow = !showOverflow
          }}
        >
          &hellip;
        </button>
        {#if showOverflow}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="overflow-backdrop"
            onclick={() => {
              showOverflow = false
            }}
          ></div>
          <div class="overflow-menu">
            {#each overflowTabs as tab (tab.id)}
              <button
                class="overflow-item"
                class:active={tab.id === currentActiveId}
                onclick={async () => {
                  await switchTab(tab.id)
                  showOverflow = false
                }}
              >
                {getTabDisplayName(tab)}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .tab-bar {
    height: 32px;
    flex-shrink: 0;
    display: flex;
    align-items: stretch;
    background: var(--c-bg-glass-light);
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .tabs-row {
    display: flex;
    align-items: stretch;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 60px;
    max-width: 180px;
    flex: 1 1 120px;
    padding: 0 8px;
    border: none;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    border-right: 1px solid var(--c-border-subtle);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }

  .tab:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .tab.dragging {
    opacity: 0.4;
  }

  .tab.drop-target {
    background: var(--c-accent-bg);
    box-shadow: inset 0 0 0 1px var(--c-accent-muted);
  }

  .drag-active .tab {
    cursor: grabbing;
  }

  .tab.active {
    background: var(--c-bg);
    color: var(--c-text);
    box-shadow: inset 0 -1px 0 var(--c-accent);
  }

  .tab.exited {
    color: color-mix(in srgb, var(--c-blazing) 60%, transparent);
  }

  .tab.exited.active {
    color: color-mix(in srgb, var(--c-blazing) 90%, transparent);
  }

  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    text-align: left;
  }

  .tab-badge {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tab-badge.connection-badge {
    background: var(--c-warning-text);
    animation: badge-pulse 1.5s ease-in-out infinite;
  }

  .tab-badge.connection-badge.disconnected {
    background: var(--c-danger-text);
    animation: none;
  }

  .tab-status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .tab-status-dot.pulse {
    animation: badge-pulse 1.5s ease-in-out infinite;
  }

  @keyframes badge-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .tab-badge.connection-badge,
    .tab-status-dot.pulse {
      animation: none;
    }
  }

  .tab-favicon {
    flex-shrink: 0;
    border-radius: 2px;
  }

  .tab-close {
    display: none;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    font-size: 14px;
    cursor: pointer;
    border-radius: 3px;
    padding: 0;
    line-height: 1;
    flex-shrink: 0;
  }

  .tab:hover .tab-close {
    display: flex;
  }

  .tab-close:hover {
    background: var(--c-hover-strong);
    color: var(--c-text);
  }

  .detach-indicator {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    flex-shrink: 0;
    background: var(--c-accent-bg);
    border: 1px solid var(--c-focus-ring);
    border-radius: 4px;
    color: var(--c-accent-text);
    font-size: 16px;
    font-weight: 600;
    pointer-events: none;
    margin: 2px 4px;
  }

  .overflow-wrapper {
    position: relative;
    flex-shrink: 0;
  }

  .overflow-trigger {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 14px;
    cursor: pointer;
  }

  .overflow-trigger:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .overflow-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }

  .overflow-menu {
    position: absolute;
    top: 100%;
    right: 0;
    min-width: 160px;
    background: var(--c-bg-overlay);
    backdrop-filter: blur(12px);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    padding: 4px;
    z-index: 100;
    box-shadow: var(--shadow-menu);
  }

  .overflow-item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    background: none;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
  }

  .overflow-item:hover {
    background: var(--c-active);
  }

  .overflow-item.active {
    color: var(--c-text);
    background: var(--c-hover);
  }
</style>
