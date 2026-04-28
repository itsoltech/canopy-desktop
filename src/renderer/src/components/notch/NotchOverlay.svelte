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
      none: 'var(--color-status-none)',
      idle: 'var(--color-status-idle)',
      working: 'var(--color-status-working)',
      waitingPermission: 'var(--color-status-permission)',
      error: 'var(--color-status-error)',
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
    class="notch-island relative flex flex-col mx-auto bg-notch-bg rounded-b-3xl overflow-hidden cursor-default transition-all duration-500 ease-notch-overshoot motion-reduce:duration-0"
    class:rounded-b-4xl={showExpanded}
    class:shadow-notch={showExpanded}
    style:width="{showExpanded ? 480 : collapsedWidth}px"
    style:height="{showExpanded ? expandedHeight : collapsedHeight}px"
    onmouseenter={handleMouseEnter}
    onmouseleave={handleMouseLeave}
  >
    <div class="flex items-center justify-center flex-shrink-0" style:height="{collapsedHeight}px">
      <span
        class="flex items-center justify-center w-10 flex-shrink-0 transition-colors duration-slow"
        style:color={statusColor}
      >
        {#if aggregateStatus === 'working'}
          <span class="flex animate-spin-slow motion-reduce:animate-none"><Loader size={15} /></span
          >
        {:else if aggregateStatus === 'waitingPermission'}
          <ShieldAlert size={15} />
        {:else}
          <TerminalSquare size={15} />
        {/if}
      </span>
      <span
        class="relative flex-shrink-0 transition-all duration-500 ease-notch-overshoot motion-reduce:duration-0"
        style:width="{gapWidth}px"
      ></span>
      <span
        class="flex items-center justify-center w-10 flex-shrink-0 transition-colors duration-slow"
        style:color={statusColor}
      >
        <Sparkles size={14} />
      </span>
    </div>

    <div class="notch-content px-1.5 pb-1.5 flex-1 min-h-0 overflow-y-auto">
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

<!--
  