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

  const sectionLabelCls = 'text-2xs font-semibold tracking-[0.5px] uppercase text-text-faint m-0'
  const infoGridCls = 'grid grid-cols-[auto_1fr] gap-x-3 gap-y-[3px] text-sm'
  const infoKeyCls = 'text-text-muted'
  const infoValCls = 'text-text-secondary'
</script>

<div class="flex flex-col gap-3 p-3" data-status={statusClass}>
  <h3 class="text-2xs font-semibold tracking-[1px] uppercase text-text-faint m-0">
    {inspectorTitle}
  </h3>

  <!-- Status -->
  <div class="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-border-subtle">
    <span class="status-dot w-2 h-2 rounded-full flex-shrink-0 bg-text-faint"></span>
    <span class="text-sm text-text">{statusText}</span>
  </div>

  <!-- Context Bar -->
  {#if state.contextPercent != null}
    <div class="flex flex-col gap-1">
      <div class="flex justify-between items-baseline">
        <span class={sectionLabelCls}>Context</span>
        <span class="text-xs text-text-secondary"
          >{Math.round(state.contextPercent)}% of {formatContextSize(state.contextSize)}</span
        >
      </div>
      <div class="h-1 rounded-xs bg-active overflow-hidden">
        <div
          class="h-full rounded-xs transition-[width] duration-slow {contextBarClass}"
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
      <h4 class={sectionLabelCls}>Tasks ({taskCounts.done}/{taskCounts.total})</h4>
      <div class="flex flex-col gap-0.5 max-h-[200px] overflow-y-auto">
        {#each visibleTasks as task (task.id)}
          <div class="flex items-center gap-1.5 py-0.5">
            {#if task.status === 'completed'}
              <span class="text-2xs w-3 flex-shrink-0 text-center text-success">✓</span>
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
      <h4 class={sectionLabelCls}>Agents ({state.activeSubagents.length})</h4>
      <div class="flex flex-col gap-0.5 max-h-[200px] overflow-y-auto">
        {#each state.activeSubagents as agent (agent.agentId)}
          <div class="flex items-center gap-1.5 py-0.5">
            <span class="w-1.5 h-1.5 rounded-full bg-focus-ring flex-shrink-0"></span>
            <span
              class="text-sm text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap flex-1"
              >{agent.agentType}</span
            >
            <span class="text-2xs text-text-faint flex-shrink-0">({agent.agentId.slice(0, 8)})</span
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
      <div class="flex flex-col gap-0.5 max-h-[200px] overflow-y-auto">
        {#each reversedNotifications as notif (notif.timestamp)}
          <div class="flex items-baseline justify-between gap-2 py-0.5">
            <span
              class="text-xs text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap flex-1"
              >{notif.message || notif.title}</span
            >
            <span class="text-2xs text-text-faint flex-shrink-0"
              >{relativeTime(notif.timestamp)}</span
            >
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<!-- data-status drives the dot color/animation; pulse keyframes are easier here than as utilities. -->
<style>
  [data-status='idle'] .status-dot {
    background: var(--color-success);
  }

  [data-status='active'] .status-dot {
    background: var(--color-accent-text);
    animation: pulse-dot 1.5s ease-in-out infinite;
  }

  [data-status='permission'] .status-dot {
    background: var(--color-warning-text);
    animation: pulse-dot 1s ease-in-out infinite;
  }

  [data-status='error'] .status-dot {
    background: var(--color-danger-text);
  }

  @keyframes pulse-dot {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .status-dot {
      animation: none !important;
    }
  }
</style>
