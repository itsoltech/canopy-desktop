<script lang="ts">
  import { onDestroy } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { TerminalSquare, Loader, ShieldAlert, Sparkles } from 'lucide-svelte'
  import NotchNotificationRow from './NotchNotificationRow.svelte'

  interface NotchSession {
    ptySessionId: string
    windowId: number
    workspaceName: string
    branch: string | null
    status:
      | 'idle'
      | 'thinking'
      | 'toolCalling'
      | 'compacting'
      | 'waitingPermission'
      | 'error'
      | 'ended'
    toolName?: string
    detail?: string
  }

  interface NotchState {
    sessions: NotchSession[]
    notchWidth: number
    notchHeight: number
    peekSessionIds?: string[]
  }

  let state: NotchState = $state({ sessions: [], notchWidth: 200, notchHeight: 37 })
  let isExpanded = $state(false)
  let isHovered = $state(false)
  let peekLocked = $state(false)
  let peekSessionIds = new SvelteSet<string>()
  let collapseTimer: ReturnType<typeof setTimeout> | null = null
  let peekTimer: ReturnType<typeof setTimeout> | null = null

  const isPeeking = $derived(peekSessionIds.size > 0)

  $effect(() => {
    return window.notchApi.onStateUpdate((s: NotchState) => {
      state = s

      if (s.peekSessionIds && s.peekSessionIds.length > 0 && !isHovered) {
        for (const id of s.peekSessionIds) peekSessionIds.add(id)

        if (peekTimer) clearTimeout(peekTimer)
        peekTimer = setTimeout(() => {
          peekSessionIds.clear()
          peekTimer = null
          if (!isHovered) window.notchApi.setMouseIgnore(true)
        }, 4000)
      }
    })
  })

  // Aggregate status: what's the "highest priority" across all sessions
  const aggregateStatus = $derived.by(() => {
    const s = state.sessions
    if (s.length === 0) return 'none'
    if (s.some((x) => x.status === 'waitingPermission')) return 'waitingPermission'
    if (s.some((x) => x.status === 'error')) return 'error'
    if (s.some((x) => x.status === 'thinking' || x.status === 'toolCalling')) return 'working'
    if (s.some((x) => x.status === 'compacting')) return 'working'
    return 'idle'
  })

  const statusColor = $derived(
    {
      none: '#555',
      idle: '#4ade80',
      working: '#f59e0b',
      waitingPermission: '#f87171',
      error: '#f87171',
    }[aggregateStatus],
  )

  const showExpanded = $derived(isExpanded || isPeeking)
  const collapsedWidth = $derived(state.notchWidth + 80)
  const collapsedHeight = $derived(state.notchHeight)
  const visibleSessions = $derived(
    isPeeking || peekLocked
      ? state.sessions.filter((s) => peekSessionIds.has(s.ptySessionId))
      : state.sessions,
  )
  const ROW_HEIGHT = 48
  const CONTENT_PADDING = 6
  const MAX_VISIBLE_ROWS = 12
  const visibleItemCount = $derived(Math.min(visibleSessions.length, MAX_VISIBLE_ROWS))
  const expandedHeight = $derived(collapsedHeight + visibleItemCount * ROW_HEIGHT + CONTENT_PADDING)
  const gapWidth = $derived(showExpanded ? 480 - 80 : state.notchWidth)

  function handleMouseEnter(): void {
    if (collapseTimer) {
      clearTimeout(collapseTimer)
      collapseTimer = null
    }
    if (isPeeking) {
      // First hover during a peek: keep showing only peeked sessions
      if (peekTimer) {
        clearTimeout(peekTimer)
        peekTimer = null
      }
      peekLocked = true
    }
    isHovered = true
    window.notchApi.setMouseIgnore(false)
    isExpanded = true
  }

  function handleMouseLeave(): void {
    isHovered = false
    collapseTimer = setTimeout(() => {
      isExpanded = false
      if (peekLocked) {
        peekLocked = false
        peekSessionIds.clear()
      }
      window.notchApi.setMouseIgnore(true)
      collapseTimer = null
    }, 300)
  }

  onDestroy(() => {
    if (collapseTimer) clearTimeout(collapseTimer)
    if (peekTimer) clearTimeout(peekTimer)
  })

  function handleRowClick(session: NotchSession): void {
    window.notchApi.focusSession(session.windowId, session.ptySessionId)
  }
</script>

{#if state.sessions.length > 0}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="island"
    class:expanded={showExpanded}
    style:width="{showExpanded ? 480 : collapsedWidth}px"
    style:height="{showExpanded ? expandedHeight : collapsedHeight}px"
    onmouseenter={handleMouseEnter}
    onmouseleave={handleMouseLeave}
  >
    <div class="header" style:height="{collapsedHeight}px">
      <span class="wing left" style:color={statusColor}>
        {#if aggregateStatus === 'working'}
          <span class="spin"><Loader size={15} /></span>
        {:else if aggregateStatus === 'waitingPermission'}
          <ShieldAlert size={15} />
        {:else}
          <TerminalSquare size={15} />
        {/if}
      </span>
      <span class="notch-gap" style:width="{gapWidth}px"></span>
      <span class="wing right" style:color={statusColor}>
        <Sparkles size={14} />
      </span>
    </div>

    <div class="content">
      {#each visibleSessions as session (session.ptySessionId)}
        <NotchNotificationRow
          {session}
          highlight={peekSessionIds.has(session.ptySessionId)}
          onclick={() => handleRowClick(session)}
        />
      {/each}
    </div>
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    background: transparent;
    overflow: hidden;
    font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif;
    -webkit-font-smoothing: antialiased;
    -webkit-user-select: none;
    user-select: none;
  }

  .island {
    position: relative;
    display: flex;
    flex-direction: column;
    margin: 0 auto;
    background: #000;
    border-radius: 0 0 16px 16px;
    overflow: hidden;
    cursor: default;
    contain: layout style;
    will-change: width, height, box-shadow;
    transition:
      width 0.5s cubic-bezier(0.175, 0.885, 0.32, 1.1),
      height 0.5s cubic-bezier(0.175, 0.885, 0.32, 1.1),
      border-radius 0.35s cubic-bezier(0.175, 0.885, 0.32, 1),
      box-shadow 0.4s ease-out;
  }

  .island.expanded {
    border-radius: 0 0 24px 24px;
    box-shadow: 0 8px 40px rgba(0, 0, 0, 0.55);
  }

  /* Concave ears */
  .island::before,
  .island::after {
    content: '';
    position: absolute;
    top: 0;
    width: 13px;
    height: 13px;
    background: transparent;
    z-index: 1;
  }

  .island::before {
    left: -13px;
    border-top-right-radius: 6px;
    box-shadow: 5px 0 #000;
  }

  .island::after {
    right: -13px;
    border-top-left-radius: 6px;
    box-shadow: -5px 0 #000;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .wing {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    flex-shrink: 0;
    transition: color 0.3s ease;
  }

  .notch-gap {
    position: relative;
    flex-shrink: 0;
    transition: width 0.5s cubic-bezier(0.175, 0.885, 0.32, 1.1);
  }

  .spin {
    display: flex;
    animation: rotate 1.5s linear infinite;
  }

  @keyframes rotate {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .content {
    padding: 0 0 6px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .content::-webkit-scrollbar {
    width: 4px;
  }

  .content::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.15); /* fixed: notch overlay always black */
    border-radius: 2px;
  }

  .content::-webkit-scrollbar-track {
    background: transparent;
  }

  @media (prefers-reduced-motion: reduce) {
    .island {
      transition-duration: 0s;
    }
    .spin {
      animation: none;
    }
    .notch-gap {
      transition-duration: 0s;
    }
  }
</style>
