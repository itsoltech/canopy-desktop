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
        ? 'ctx-red'
        : state.contextPercent >= 70
          ? 'ctx-yellow'
          : 'ctx-green',
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
</script>

<div class="inspector">
  <h3 class="inspector-title">{inspectorTitle}</h3>

  <!-- Status -->
  <div class="status-badge {statusClass}">
    <span class="status-dot"></span>
    <span class="status-text">{statusText}</span>
  </div>

  <!-- Context Bar -->
  {#if state.contextPercent != null}
    <div class="context-section">
      <div class="context-header">
        <span class="context-label">Context</span>
        <span class="context-value"
          >{Math.round(state.contextPercent)}% of {formatContextSize(state.contextSize)}</span
        >
      </div>
      <div class="context-track">
        <div
          class="context-fill {contextBarClass}"
          style="width: {Math.min(state.contextPercent, 100)}%"
        ></div>
      </div>
    </div>
  {/if}

  <!-- Session Info -->
  <div class="section">
    <h4 class="section-label">Session</h4>
    <div class="info-grid">
      {#if state.model}
        <span class="info-key">Model</span>
        <span class="info-val">{state.model}</span>
      {/if}
      {#if state.permissionMode}
        <span class="info-key">Mode</span>
        <span class="info-val">{state.permissionMode}</span>
      {/if}
      {#if state.costUsd != null}
        <span class="info-key">Cost</span>
        <span class="info-val">{formatCost(state.costUsd)}</span>
      {/if}
      {#if state.durationMs != null}
        <span class="info-key">Duration</span>
        <span class="info-val">{formatDuration(state.durationMs)}</span>
      {/if}
      {#if state.linesAdded != null || state.linesRemoved != null}
        <span class="info-key">Lines</span>
        <span class="info-val">+{state.linesAdded ?? 0} -{state.linesRemoved ?? 0}</span>
      {/if}
      {#if state.toolCallCount > 0}
        <span class="info-key">Tool calls</span>
        <span class="info-val">{state.toolCallCount}</span>
      {/if}
      {#if state.compactCount > 0}
        <span class="info-key">Compactions</span>
        <span class="info-val">{state.compactCount}</span>
      {/if}
      {#if state.version}
        <span class="info-key">Version</span>
        <span class="info-val">{state.version}</span>
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
    <div class="section">
      <h4 class="section-label">Tasks ({taskCounts.done}/{taskCounts.total})</h4>
      <div class="item-list">
        {#each visibleTasks as task (task.id)}
          <div class="item-row">
            {#if task.status === 'completed'}
              <span class="task-icon task-done">✓</span>
            {:else if task.status === 'in_progress'}
              <span class="task-icon task-active">▶</span>
            {:else}
              <span class="task-icon task-pending">○</span>
            {/if}
            <span class="item-text" class:task-completed={task.status === 'completed'}
              >{task.subject}</span
            >
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Agents -->
  {#if state.activeSubagents.length > 0}
    <div class="section">
      <h4 class="section-label">Agents ({state.activeSubagents.length})</h4>
      <div class="item-list">
        {#each state.activeSubagents as agent (agent.agentId)}
          <div class="item-row">
            <span class="agent-dot"></span>
            <span class="item-text">{agent.agentType}</span>
            <span class="item-dim">({agent.agentId.slice(0, 8)})</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Notifications -->
  {#if reversedNotifications.length > 0}
    <div class="section">
      <h4 class="section-label">Notifications</h4>
      <div class="item-list">
        {#each reversedNotifications as notif (notif.timestamp)}
          <div class="notif-row">
            <span class="notif-msg">{notif.message || notif.title}</span>
            <span class="notif-time">{relativeTime(notif.timestamp)}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  /* Context bar */
  .context-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .context-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .context-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--color-text-faint);
  }

  .context-value {
    font-size: 11px;
    color: var(--color-text-secondary);
  }

  .context-track {
    height: 4px;
    border-radius: 2px;
    background: var(--color-active);
    overflow: hidden;
  }

  .context-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .context-fill.ctx-green {
    background: var(--color-success);
  }

  .context-fill.ctx-yellow {
    background: var(--color-warning);
  }

  .context-fill.ctx-red {
    background: var(--color-danger-text);
  }

  .inspector {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
  }

  .inspector-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--color-text-faint);
    margin: 0;
  }

  /* Status */
  .status-badge {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--color-border-subtle);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--color-text-faint);
  }

  .status-badge.idle .status-dot {
    background: var(--color-success);
  }

  .status-badge.active .status-dot {
    background: var(--color-accent-text);
    animation: pulse-dot 1.5s ease-in-out infinite;
  }

  .status-badge.permission .status-dot {
    background: var(--color-warning-text);
    animation: pulse-dot 1s ease-in-out infinite;
  }

  .status-badge.error .status-dot {
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

  .status-text {
    font-size: 12px;
    color: var(--color-text);
  }

  /* Sections */
  .section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .section-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--color-text-faint);
    margin: 0;
  }

  /* Info grid */
  .info-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 12px;
    font-size: 12px;
  }

  .info-key {
    color: var(--color-text-muted);
  }

  .info-val {
    color: var(--color-text-secondary);
  }

  /* Item lists */
  .item-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 200px;
    overflow-y: auto;
  }

  .item-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0;
  }

  .task-icon {
    font-size: 10px;
    width: 12px;
    flex-shrink: 0;
    text-align: center;
  }

  .task-done {
    color: var(--color-success);
  }

  .task-active {
    color: var(--color-accent-text);
  }

  .task-pending {
    color: var(--color-text-faint);
  }

  .task-completed {
    text-decoration: line-through;
    opacity: 0.5;
  }

  .item-text {
    font-size: 12px;
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .item-dim {
    font-size: 10px;
    color: var(--color-text-faint);
    flex-shrink: 0;
  }

  .agent-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-focus-ring);
    flex-shrink: 0;
  }

  /* Notifications */
  .notif-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    padding: 2px 0;
  }

  .notif-msg {
    font-size: 11px;
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .notif-time {
    font-size: 10px;
    color: var(--color-text-faint);
    flex-shrink: 0;
  }
</style>
