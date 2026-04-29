<script lang="ts">
  import { match, P } from 'ts-pattern'
  import type { AgentSessionState } from '../../lib/agents/agentState.svelte'
  import ClaudeExtras from './ClaudeExtras.svelte'
  import OpenCodeExtras from './OpenCodeExtras.svelte'
  import CodexExtras from './CodexExtras.svelte'

  let {
    state,
  }: {
    state: AgentSessionState
  } = $props()

  let inspectorTitle = $derived(
    state.agentType.charAt(0).toUpperCase() + state.agentType.slice(1) + ' Inspector',
  )

  let statusText = $derived(
    match(state.status)
      .with({ type: 'inactive' }, () => 'Inactive')
      .with({ type: 'starting' }, () => 'Starting...')
      .with({ type: 'idle' }, () => 'Idle')
      .with({ type: 'thinking' }, () => 'Thinking...')
      .with({ type: 'compacting' }, () => 'Compacting...')
      .with({ type: 'toolCalling' }, (s) => `Calling ${s.toolName}`)
      .with({ type: 'waitingPermission' }, () => 'Permission Required')
      .with({ type: 'error' }, () => 'Error')
      .with({ type: 'ended' }, () => 'Ended')
      .exhaustive(),
  )

  let statusClass = $derived(
    match(state.status.type)
      .with('waitingPermission', () => 'permission')
      .with('error', () => 'error')
      .with(P.union('toolCalling', 'thinking', 'compacting'), () => 'active')
      .with('idle', () => 'idle')
      .otherwise(() => 'dim'),
  )

  let statusDotTone = $derived(
    match(statusClass)
      .with('permission', () => 'bg-warning-text animate-badge-pulse motion-reduce:animate-none')
      .with('error', () => 'bg-danger-text')
      .with('active', () => 'bg-accent-text animate-badge-pulse motion-reduce:animate-none')
      .with('idle', () => 'bg-success')
      .otherwise(() => 'bg-text-faint'),
  )

  function relativeTime(ts: number): string {
    const diff = Date.now() - ts
    const seconds = Math.floor(diff / 1000)
    if (seconds < 60) return 'just now'
    const minutes = Math.floor(seconds / 60)
    if (minutes < 60) return `${minutes}m ago`
    const hours = Math.floor(minutes / 60)
    return `${hours}h ago`
  }

  let reversedNotifications = $derived([...state.notifications].reverse())

  let visibleTasks = $derived(state.tasks.filter((t) => t.status !== 'deleted'))
  let taskCounts = $derived.by(() => {
    const done = visibleTasks.filter((t) => t.status === 'completed').length
    const total = visibleTasks.length
    return { done, total }
  })

  let contextBarClass = $derived(
    state.contextPercent == null
      ? ''
      : state.contextPercent >= 90
        ? 'bg-danger-text'
        : state.contextPercent >= 70
          ? 'bg-warning'
          : 'bg-success',
  )

  function formatContextSize(size: number | null): string {
    if (size == null) return ''
    if (size >= 1000000) return `${(size / 1000000).toFixed(0)}M`
    return `${Math.round(size / 1000)}k`
  }

  function formatCost(cost: number | null): string {
    if (cost == null) return ''
    return `$${cost.toFixed(2)}`
  }

  function formatDuration(ms: number | null): string {
    if (ms == null) return ''
    const seconds = Math.floor(ms / 1000)
    if (seconds < 60) return `${seconds}s`
    const minutes = Math.floor(seconds / 60)
    const secs = seconds % 60
    if (minutes < 60) return `${minutes}m ${secs}s`
    const hours = Math.floor(minutes / 60)
    return `${hours}h ${minutes % 60}m`
  }

  const sectionLabelCls =
    'text-2xs font-semibold tracking-caps-looser uppercase text-text-faint leading-tight m-0'
  const infoGridCls = 'grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm'
  const infoKeyCls = 'text-text-faint'
  const infoValCls = 'text-text-secondary'
</script>

<div class="flex flex-col gap-4 p-3" data-status={statusClass}>
  <h3
    class="text-2xs font-semibold tracking-caps-looser uppercase text-text-faint leading-tight m-0"
  >
    {inspectorTitle}
  </h3>

  <!-- Status -->
  <div class="flex items-center gap-2 h-7 px-2.5 rounded-md bg-border-subtle">
    <span class="size-2 rounded-full flex-shrink-0 {statusDotTone}" aria-hidden="true"></span>
    <span class="text-sm text-text">{statusText}</span>
  </div>

  <!-- Context Bar -->
  {#if state.contextPercent != null}
    <div class="flex flex-col gap-1.5">
      <div class="flex justify-between items-baseline">
        <span class={sectionLabelCls}>Context</span>
        <span class="text-2xs font-mono tabular-nums text-text-secondary"
          >{Math.round(state.contextPercent)}% of {formatContextSize(state.contextSize)}</span
        >
      </div>
      <div class="h-1.5 rounded-sm bg-border-subtle overflow-hidden">
        <div
          class="h-full rounded-sm transition-[width] duration-slow {contextBarClass}"
          style="width: {Math.min(state.contextPercent, 100)}%"
        ></div>
      </div>
    </div>
  {/if}

  <!-- Session Info -->
  <div class="flex flex-col gap-1.5">
    <h4 class={sectionLabelCls}>Session</h4>
    <div class={infoGridCls}>
      {#if state.model}
        <span class={infoKeyCls}>Model</span>
        <span class={infoValCls}>{state.model}</span>
      {/if}
      {#if state.permissionMode}
        <span class={infoKeyCls}>Mode</span>
        <span class={infoValCls}>{state.permissionMode}</span>
      {/if}
      {#if state.costUsd != null}
        <span class={infoKeyCls}>Cost</span>
        <span class={infoValCls}>{formatCost(state.costUsd)}</span>
      {/if}
      {#if state.durationMs != null}
        <span class={infoKeyCls}>Duration</span>
        <span class={infoValCls}>{formatDuration(state.durationMs)}</span>
      {/if}
      {#if state.linesAdded != null || state.linesRemoved != null}
        <span class={infoKeyCls}>Lines</span>
        <span class={infoValCls}>+{state.linesAdded ?? 0} -{state.linesRemoved ?? 0}</span>
      {/if}
      {#if state.toolCallCount > 0}
        <span class={infoKeyCls}>Tool calls</span>
        <span class={infoValCls}>{state.toolCallCount}</span>
      {/if}
      {#if state.compactCount > 0}
        <span class={infoKeyCls}>Compactions</span>
        <span class={infoValCls}>{state.compactCount}</span>
      {/if}
      {#if state.version}
        <span class={infoKeyCls}>Version</span>
        <span class={infoValCls}>{state.version}</span>
      {/if}
    </div>
  </div>

  <!-- Agent-specific extras -->
  {#if state.agentType === 'claude'}
    <ClaudeExtras extra={state.extra} />
  {:else if state.agentType === 'opencode'}
    <OpenCodeExtras extra={state.extra} />
  {:else if state.agentType === 'codex'}
    <CodexExtras extra={state.extra} />
  {/if}

  <!-- Tasks -->
  {#if visibleTasks.length > 0}
    <div class="flex flex-col gap-1.5">
      <div class="flex items-baseline justify-between">
        <h4 class={sectionLabelCls}>Tasks</h4>
        <span class="text-2xs font-mono tabular-nums text-text-faint"
          >{taskCounts.done}/{taskCounts.total}</span
        >
      </div>
      <div class="flex flex-col max-h-[200px] overflow-y-auto -mx-1">
        {#each visibleTasks as task (task.id)}
          <div
            class="flex items-center gap-2 h-6 px-1 rounded-sm transition-colors duration-fast hover:bg-hover"
          >
            {#if task.status === 'completed'}
              <span class="text-2xs w-3 flex-shrink-0 text-center text-success-text">✓</span>
            {:else if task.status === 'in_progress'}
              <span class="text-2xs w-3 flex-shrink-0 text-center text-accent-text">▶</span>
            {:else}
              <span class="text-2xs w-3 flex-shrink-0 text-center text-text-faint">○</span>
            {/if}
            <span
              class="text-sm text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap flex-1"
              class:line-through={task.status === 'completed'}
              class:opacity-50={task.status === 'completed'}>{task.subject}</span
            >
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Agents -->
  {#if state.activeSubagents.length > 0}
    <div class="flex flex-col gap-1.5">
      <div class="flex items-baseline justify-between">
        <h4 class={sectionLabelCls}>Agents</h4>
        <span class="text-2xs font-mono tabular-nums text-text-faint"
          >{state.activeSubagents.length}</span
        >
      </div>
      <div class="flex flex-col max-h-[200px] overflow-y-auto -mx-1">
        {#each state.activeSubagents as agent (agent.agentId)}
          <div
            class="flex items-center gap-2 h-6 px-1 rounded-sm transition-colors duration-fast hover:bg-hover"
          >
            <span class="size-1.5 rounded-full bg-focus-ring flex-shrink-0"></span>
            <span
              class="text-sm text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap flex-1"
              >{agent.agentType}</span
            >
            <span class="text-2xs font-mono text-text-faint flex-shrink-0"
              >{agent.agentId.slice(0, 8)}</span
            >
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Notifications -->
  {#if reversedNotifications.length > 0}
    <div class="flex flex-col gap-1.5">
      <h4 class={sectionLabelCls}>Notifications</h4>
      <div class="flex flex-col max-h-[200px] overflow-y-auto -mx-1">
        {#each reversedNotifications as notif (notif.timestamp)}
          <div
            class="flex items-baseline justify-between gap-2 px-1 py-1 rounded-sm transition-colors duration-fast hover:bg-hover"
          >
            <span
              class="text-xs text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap flex-1"
              >{notif.message || notif.title}</span
            >
            <span
              class="inline-flex items-center h-4 px-1 rounded-sm bg-border-subtle text-text-faint text-2xs font-mono tabular-nums leading-none flex-shrink-0"
              >{relativeTime(notif.timestamp)}</span
            >
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
