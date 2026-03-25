<script lang="ts">
  import type { ClaudeSessionState } from '../../lib/claude/claudeState.svelte'

  let {
    state,
  }: {
    state: ClaudeSessionState
  } = $props()

  let statusText = $derived.by(() => {
    switch (state.status.type) {
      case 'inactive':
        return 'Inactive'
      case 'starting':
        return 'Starting...'
      case 'idle':
        return 'Idle'
      case 'thinking':
        return 'Thinking...'
      case 'compacting':
        return 'Compacting...'
      case 'toolCalling':
        return `Calling ${state.status.toolName}`
      case 'waitingPermission':
        return 'Permission Required'
      case 'error':
        return `Error`
      case 'ended':
        return 'Ended'
    }
  })

  let statusClass = $derived(
    state.status.type === 'waitingPermission'
      ? 'permission'
      : state.status.type === 'error'
        ? 'error'
        : state.status.type === 'toolCalling' ||
            state.status.type === 'thinking' ||
            state.status.type === 'compacting'
          ? 'active'
          : state.status.type === 'idle'
            ? 'idle'
            : 'dim',
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

  function formatResetTime(resetsAt: number | null): string {
    if (resetsAt == null) return ''
    const diff = resetsAt - Date.now()
    if (diff <= 0) return ''
    const minutes = Math.floor(diff / 60000)
    if (minutes < 1) return 'in <1min'
    const hours = Math.floor(minutes / 60)
    const mins = minutes % 60
    const days = Math.floor(hours / 24)
    const remHours = hours % 24
    if (days > 0) {
      if (remHours === 0 && mins === 0) return `in ${days}d`
      if (mins === 0) return `in ${days}d ${remHours}h`
      return `in ${days}d ${remHours}h ${mins}min`
    }
    if (hours === 0) return `in ${mins}min`
    if (mins === 0) return `in ${hours}h`
    return `in ${hours}h ${mins}min`
  }

  function rateLimitBarClass(pct: number): string {
    if (pct >= 90) return 'ctx-red'
    if (pct >= 70) return 'ctx-yellow'
    return 'ctx-green'
  }
</script>

<aside class="inspector">
  <h3 class="inspector-title">Claude Inspector</h3>

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

  <!-- Rate Limits -->
  {#if state.rateLimitFiveHour != null || state.rateLimitSevenDay != null}
    <div class="section">
      <h4 class="section-label">Rate Limits</h4>
      {#if state.rateLimitFiveHour != null}
        <div class="rate-limit-row">
          <div class="rate-limit-header">
            <span class="rate-limit-label">5h window</span>
            <span class="rate-limit-meta"
              >{Math.round(
                100 - state.rateLimitFiveHour,
              )}%{#if formatResetTime(state.rateLimitFiveHourResetsAt)}{' '}
                <span class="rate-limit-reset"
                  >{formatResetTime(state.rateLimitFiveHourResetsAt)}</span
                >{/if}</span
            >
          </div>
          <div class="context-track">
            <div
              class="context-fill {rateLimitBarClass(state.rateLimitFiveHour)}"
              style="width: {Math.max(100 - state.rateLimitFiveHour, 0)}%"
            ></div>
          </div>
        </div>
      {/if}
      {#if state.rateLimitSevenDay != null}
        <div class="rate-limit-row">
          <div class="rate-limit-header">
            <span class="rate-limit-label">7d window</span>
            <span class="rate-limit-meta"
              >{Math.round(
                100 - state.rateLimitSevenDay,
              )}%{#if formatResetTime(state.rateLimitSevenDayResetsAt)}{' '}
                <span class="rate-limit-reset"
                  >{formatResetTime(state.rateLimitSevenDayResetsAt)}</span
                >{/if}</span
            >
          </div>
          <div class="context-track">
            <div
              class="context-fill {rateLimitBarClass(state.rateLimitSevenDay)}"
              style="width: {Math.max(100 - state.rateLimitSevenDay, 0)}%"
            ></div>
          </div>
        </div>
      {/if}
    </div>
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
</aside>

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
    color: rgba(255, 255, 255, 0.25);
  }

  .context-value {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
  }

  .context-track {
    height: 4px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }

  .context-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .context-fill.ctx-green {
    background: rgba(100, 200, 100, 0.6);
  }

  .context-fill.ctx-yellow {
    background: rgba(255, 200, 50, 0.7);
  }

  .context-fill.ctx-red {
    background: rgba(255, 100, 100, 0.7);
  }

  .inspector {
    width: 280px;
    min-width: 280px;
    height: 100%;
    background: rgba(30, 30, 30, 0.75);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border-left: 1px solid rgba(255, 255, 255, 0.06);
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .inspector-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.3);
    margin: 0;
  }

  /* Status */
  .status-badge {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.04);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    background: rgba(255, 255, 255, 0.2);
  }

  .status-badge.idle .status-dot {
    background: rgba(100, 200, 100, 0.6);
  }

  .status-badge.active .status-dot {
    background: rgba(116, 192, 252, 0.8);
    animation: pulse-dot 1.5s ease-in-out infinite;
  }

  .status-badge.permission .status-dot {
    background: rgba(255, 160, 50, 0.9);
    animation: pulse-dot 1s ease-in-out infinite;
  }

  .status-badge.error .status-dot {
    background: rgba(255, 100, 100, 0.8);
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

  .status-text {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
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
    color: rgba(255, 255, 255, 0.25);
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
    color: rgba(255, 255, 255, 0.35);
  }

  .info-val {
    color: rgba(255, 255, 255, 0.6);
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
    color: rgba(100, 200, 100, 0.6);
  }

  .task-active {
    color: rgba(116, 192, 252, 0.8);
  }

  .task-pending {
    color: rgba(255, 255, 255, 0.25);
  }

  .task-completed {
    text-decoration: line-through;
    opacity: 0.5;
  }

  .item-text {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.55);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .item-dim {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.2);
    flex-shrink: 0;
  }

  .agent-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: rgba(116, 192, 252, 0.6);
    flex-shrink: 0;
  }

  /* Rate limit rows */
  .rate-limit-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .rate-limit-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .rate-limit-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.35);
  }

  .rate-limit-meta {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
  }

  .rate-limit-reset {
    color: rgba(255, 255, 255, 0.3);
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
    color: rgba(255, 255, 255, 0.5);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .notif-time {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.2);
    flex-shrink: 0;
  }
</style>
