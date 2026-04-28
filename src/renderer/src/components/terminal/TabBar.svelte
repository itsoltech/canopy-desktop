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
    isTabDirty,
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
      const sessionIds = tabs
        .flatMap((tab) => allPanes(tab.rootSplit))
        .filter((p) => !agentSessions[p.sessionId] && p.isRunning)
        .map((p) => p.sessionId)

      if (sessionIds.length === 0) {
        shellBusyState = {}
        return
      }

      try {
        shellBusyState = await window.api.hasChildProcesses(sessionIds)
      } catch {
        shellBusyState = {}
      }
    }

    void pollShellBusy()
    shellPollTimer = setInterval(() => void pollShellBusy(), 2000)
  })

  // Cleanup returned from an async onMount is silently dropped by Svelte,
  // so register the drag listener teardown via $effect instead. If the
  // tab bar unmounts mid-drag (window close, workspace switch), this runs
  // and detaches the window-level pointermove/pointerup handlers that
  // would otherwise leak with their closures.
  $effect(() => {
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
      return { color: 'var(--color-warning)', pulse: true, label: 'Permission required' }
    if (badge === 'unread')
      return { color: 'var(--color-accent)', pulse: true, label: 'Unread activity' }

    const panes = allPanes(tab.rootSplit)

    // If every pane has exited, surface that instead of falling through to an
    // "idle" state — keeps the dot in sync with the `.tab.exited` text color.
    if (panes.length > 0 && panes.every((p) => !p.isRunning)) {
      return { color: 'var(--color-blazing)', pulse: false, label: 'Exited' }
    }

    let priority = 0

    for (const p of panes) {
      if (!p.isRunning) continue
      const session = agentSessions[p.sessionId]
      if (session) {
        const t = session.status.type
        if (t === 'waitingPermission')
          return { color: 'var(--color-warning)', pulse: true, label: 'Permission required' }
        if (t === 'error') priority = Math.max(priority, 5)
        else if (t === 'thinking' || t === 'toolCalling' || t === 'compacting' || t === 'starting')
          priority = Math.max(priority, 4)
        else if (t === 'idle' || t === 'ended') priority = Math.max(priority, 3)
      } else {
        priority = Math.max(priority, shellBusyState[p.sessionId] ? 2 : 1)
      }
    }

    if (priority === 5) return { color: 'var(--color-danger)', pulse: false, label: 'Agent error' }
    if (priority === 4) return { color: 'var(--color-accent)', pulse: true, label: 'Agent working' }
    if (priority === 3) return { color: 'var(--color-success)', pulse: false, label: 'Agent idle' }
    if (priority === 2) return { color: 'var(--color-accent)', pulse: true, label: 'Shell running' }
    if (priority === 1) return { color: 'var(--color-success)', pulse: false, label: 'Shell idle' }
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
  let shellPollTimer: ReturnType<typeof setInterval> | undefined
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
  <div
    class="h-tab-bar flex-shrink-0 flex items-stretch bg-bg-glass-light border-b border-border-subtle"
    class:cursor-grabbing={dragActive}
    bind:this={containerEl}
  >
    <div
      class="flex items-stretch flex-1 min-w-0 overflow-hidden"
      role="tablist"
      aria-label="Terminal tabs"
    >
      {#each visibleTabs as tab (tab.id)}
        {@const connState = getConnectionState(tab)}
        {@const favicon = getTabFavicon(tab)}
        {@const isActiveTab = tab.id === currentActiveId}
        {@const isExited = !tab.suspended && allPanes(tab.rootSplit).some((p) => !p.isRunning)}
        {@const isDragging = dragActive && dragTabId === tab.id}
        {@const isDropTarget = dragActive && dropTargetId === tab.id}
        <div
          class="group/tab flex items-center gap-1 min-w-15 max-w-45 flex-1 basis-30 px-2 border-0 bg-transparent text-text-secondary text-xs font-inherit cursor-pointer border-r border-border-subtle transition-colors duration-fast hover:bg-hover hover:text-text"
          class:bg-bg={isActiveTab && !isDropTarget}
          class:text-text={isActiveTab}
          class:shadow-tab-active={isActiveTab && !isDropTarget}
          class:opacity-40={isDragging}
          class:bg-accent-bg={isDropTarget}
          class:shadow-tab-drop={isDropTarget}
          class:text-blazing={isExited && !isActiveTab}
          class:text-blazing-text={isExited && isActiveTab}
          data-tab-id={tab.id}
          role="tab"
          tabindex={isActiveTab ? 0 : -1}
          aria-selected={isActiveTab}
          onclick={async () => {
            if (!suppressClick) await switchTab(tab.id)
          }}
          onkeydown={async (e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault()
              await switchTab(tab.id)
            }
          }}
          onauxclick={(e) => handleMiddleClick(e, tab.id)}
          onpointerdown={(e) => handleTabPointerDown(e, tab.id)}
          title={getTabDisplayName(tab)}
        >
          {#if favicon}
            <img class="flex-shrink-0 rounded-xs" src={favicon} alt="" width="12" height="12" />
          {:else}
            {@const dot = getTabStatusDot(tab)}
            <span
              class="w-2 h-2 rounded-full flex-shrink-0 motion-reduce:animate-none"
              class:animate-badge-pulse={dot?.pulse ?? false}
              style:background={dot?.color ?? 'var(--color-text-faint)'}
              role={dot ? 'status' : undefined}
              aria-label={dot?.label ?? undefined}
              title={dot?.label ?? undefined}
            ></span>
          {/if}
          {#if isTabDirty(tab)}
            <span
              class="w-2 h-2 rounded-full bg-text-muted flex-shrink-0"
              aria-label="Unsaved changes"
              title="Unsaved changes"
            ></span>
          {/if}
          <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1 text-left"
            >{getTabDisplayName(tab)}</span
          >
          {#if connState}
            <span
              class="w-1.5 h-1.5 rounded-full flex-shrink-0 motion-reduce:animate-none"
              class:bg-warning-text={connState !== 'disconnected'}
              class:animate-badge-pulse={connState !== 'disconnected'}
              class:bg-danger-text={connState === 'disconnected'}
              role="status"
              aria-label={connState === 'disconnected' ? 'Disconnected' : 'Reconnecting'}
              title={connState === 'disconnected' ? 'Disconnected' : 'Reconnecting...'}
            ></span>
          {/if}
          <button
            class="hidden group-hover/tab:flex items-center justify-center w-4 h-4 border-0 bg-transparent text-text-muted text-lg cursor-pointer rounded-sm p-0 leading-none flex-shrink-0 hover:bg-hover-strong hover:text-text"
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
      <div
        class="flex items-center justify-center w-8 flex-shrink-0 bg-accent-bg border border-focus-ring rounded-md text-accent-text text-xl font-semibold pointer-events-none my-0.5 mx-1"
        aria-label="Drop to detach pane as new tab"
      >
        +
      </div>
    {/if}

    {#if overflowTabs.length > 0}
      <div class="relative flex-shrink-0">
        <button
          class="flex items-center justify-center w-8 h-full border-0 bg-transparent text-text-secondary text-lg cursor-pointer hover:bg-hover hover:text-text"
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
            class="fixed inset-0 z-overlay"
            onclick={() => {
              showOverflow = false
            }}
          ></div>
          <div
            class="absolute top-full right-0 min-w-40 bg-bg-overlay backdrop-blur-md border border-border rounded-lg p-1 z-popover shadow-menu"
          >
            {#each overflowTabs as tab (tab.id)}
              <button
                class="block w-full px-2.5 py-1.5 border-0 bg-transparent text-text text-sm font-inherit text-left cursor-pointer rounded-md hover:bg-active"
                class:bg-hover={tab.id === currentActiveId}
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
