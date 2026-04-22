<script lang="ts">
  import { match } from 'ts-pattern'
  import { workspaceState, projects, toggleRightPanel } from '../../lib/stores/workspace.svelte'
  import { agentSessions, type AgentSessionState } from '../../lib/agents/agentState.svelte'
  import {
    sdkSessions,
    contextPercentFor,
    contextTokensUsedFor,
  } from '../../lib/stores/sdkAgentSessions.svelte'
  import { getAllTabs, activeTabId, focusSessionByPtyId } from '../../lib/stores/tabs.svelte'
  import { findLeaf } from '../../lib/stores/splitTree'
  import { updateState, installUpdate } from '../../lib/stores/updateState.svelte'
  import { prefs } from '../../lib/stores/preferences.svelte'
  import { perfHudState, enablePerfHud, disablePerfHud } from '../../lib/stores/perfHud.svelte'
  import { showPreferences } from '../../lib/stores/dialogs.svelte'
  import { Settings } from '@lucide/svelte'

  const AI_TOOL_IDS = new Set(['claude', 'codex', 'opencode', 'gemini'])

  // --- Derived: focused pane + agent ---

  let allTabs = $derived(getAllTabs())

  let currentActiveTabId = $derived(
    workspaceState.selectedWorktreePath
      ? (activeTabId[workspaceState.selectedWorktreePath] ?? null)
      : null,
  )

  let activeTab = $derived(allTabs.find((t) => t.id === currentActiveTabId) ?? null)

  let focusedPane = $derived(
    activeTab ? findLeaf(activeTab.rootSplit, activeTab.focusedPaneId) : null,
  )

  let activeAgent: AgentSessionState | null = $derived(
    focusedPane && agentSessions[focusedPane.sessionId]
      ? agentSessions[focusedPane.sessionId]
      : null,
  )

  // --- Derived: global agent stats ---

  let allAgentEntries = $derived(Object.entries(agentSessions))
  let agentCount = $derived(allAgentEntries.length)

  type WorstStatus = 'none' | 'idle' | 'working' | 'waitingPermission' | 'error'

  let globalWorstStatus: WorstStatus = $derived.by(() => {
    let worst: WorstStatus = 'none'
    for (const [, s] of allAgentEntries) {
      const t = s.status.type
      if (t === 'waitingPermission') return 'waitingPermission'
      if (t === 'error' && worst !== 'waitingPermission') worst = 'error'
      else if ((t === 'thinking' || t === 'toolCalling' || t === 'compacting') && worst !== 'error')
        worst = 'working'
      else if (t === 'idle' && worst === 'none') worst = 'idle'
    }
    return worst
  })

  let permissionSessionId: string | null = $derived.by(() => {
    for (const [id, s] of allAgentEntries) {
      if (s.status.type === 'waitingPermission') return id
    }
    return null
  })

  // --- Derived: focused pane type ---

  type PaneKind =
    | 'agent'
    | 'sdkChat'
    | 'shell'
    | 'browser'
    | 'editor'
    | 'notes'
    | 'drawing'
    | 'none'

  let focusedPaneKind: PaneKind = $derived.by(() => {
    if (!focusedPane) return 'none'
    if (focusedPane.paneType === 'sdkChat') return 'sdkChat'
    if (focusedPane.paneType === 'browser') return 'browser'
    if (focusedPane.paneType === 'editor') return 'editor'
    if (focusedPane.paneType === 'notes') return 'notes'
    if (focusedPane.paneType === 'drawing') return 'drawing'
    if (AI_TOOL_IDS.has(focusedPane.toolId)) return 'agent'
    return 'shell'
  })

  // SDK chat panes track a separate session store (sdkSessions). Read from it
  // when the focused pane is an SDK chat — the hook-based `agentSessions`
  // store doesn't cover these conversations.
  let activeSdkSession = $derived(
    focusedPane?.paneType === 'sdkChat' && focusedPane.conversationId
      ? sdkSessions[focusedPane.conversationId]
      : null,
  )
  let sdkContextPercent = $derived(contextPercentFor(activeSdkSession ?? undefined))

  // --- Derived: worktree display ---

  let worktreeCount = $derived(workspaceState.worktrees.length)

  let worktreeName = $derived.by(() => {
    const path = workspaceState.selectedWorktreePath
    if (!path) return null
    return path.split('/').pop() || path
  })

  // --- Helpers ---

  function globalStatusLabel(status: WorstStatus): string {
    return match(status)
      .with('waitingPermission', () => 'waiting for permission')
      .with('error', () => 'error')
      .with('working', () => 'working')
      .with('idle', () => 'idle')
      .otherwise(() => '')
  }

  function statusDotColor(status: WorstStatus): string {
    return match(status)
      .with('waitingPermission', () => 'var(--c-warning-text)')
      .with('error', () => 'var(--c-danger-text)')
      .with('working', () => 'var(--c-accent-text)')
      .with('idle', () => 'var(--c-success)')
      .otherwise(() => 'var(--c-text-faint)')
  }

  function agentStatusLabel(s: AgentSessionState): string {
    return match(s.status)
      .with({ type: 'idle' }, () => 'Idle')
      .with({ type: 'thinking' }, () => 'Thinking')
      .with({ type: 'compacting' }, () => 'Compacting')
      .with({ type: 'toolCalling' }, (st) => `Tool: ${st.toolName}`)
      .with({ type: 'waitingPermission' }, (st) => `Permission: ${st.toolName}`)
      .with({ type: 'error' }, () => 'Error')
      .with({ type: 'starting' }, () => 'Starting')
      .with({ type: 'ended' }, () => 'Ended')
      .otherwise(() => '')
  }

  function contextColor(pct: number): string {
    if (pct >= 90) return 'var(--c-danger-text)'
    if (pct >= 70) return 'var(--c-warning-text)'
    return 'var(--c-success)'
  }

  function formatCost(usd: number): string {
    return `$${usd.toFixed(2)}`
  }

  function focusPermissionAgent(): void {
    if (permissionSessionId) {
      focusSessionByPtyId(permissionSessionId)
    }
  }

  function focusWorstAgent(): void {
    if (permissionSessionId) {
      focusSessionByPtyId(permissionSessionId)
      return
    }
    // Focus first error agent, then first working agent
    for (const [id, s] of allAgentEntries) {
      if (s.status.type === 'error') {
        focusSessionByPtyId(id)
        return
      }
    }
    for (const [id, s] of allAgentEntries) {
      const t = s.status.type
      if (t === 'thinking' || t === 'toolCalling' || t === 'compacting') {
        focusSessionByPtyId(id)
        return
      }
    }
  }

  let hasUpdate = $derived(updateState.status === 'ready')

  // Task progress for focused agent
  let taskProgress = $derived.by(() => {
    if (!activeAgent || activeAgent.tasks.length === 0) return null
    const total = activeAgent.tasks.filter((t) => t.status !== 'deleted').length
    const done = activeAgent.tasks.filter((t) => t.status === 'completed').length
    return { done, total }
  })

  // --- Perf HUD (status-bar CPU/RAM indicator) ---
  // Subscription is gated on the user preference. When toggled off the main
  // process stops sampling entirely, so the HUD adds zero overhead in that state.
  let perfHudEnabled = $derived(prefs['perf.hud.enabled'] === 'true')

  $effect(() => {
    if (perfHudEnabled) {
      enablePerfHud()
      return () => {
        disablePerfHud()
      }
    }
    return undefined
  })

  function openPerfHudSettings(): void {
    showPreferences('general')
  }
</script>

{#if projects.length > 0}
  <footer class="status-bar">
    <!-- LEFT: git & worktree -->
    <div class="section left">
      {#if workspaceState.isGitRepo && workspaceState.branch}
        <button
          class="status-item branch"
          aria-label="Branch: {workspaceState.branch}{workspaceState.isDirty
            ? ', uncommitted changes'
            : ''}"
          title="Branch: {workspaceState.branch}"
        >
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
            <path
              d="M5 3.25a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0Zm0 2.122a2.25 2.25 0 1 0-1 0v1.836A2.252 2.252 0 0 0 2 9.5a2.25 2.25 0 1 0 3.163.132l2.382-2.382A1.75 1.75 0 0 1 8.783 6.75h1.467a2.25 2.25 0 1 0 0-1.5H8.783a3.25 3.25 0 0 0-2.299.952L4.5 8.186V5.372ZM4.25 12a.75.75 0 1 1 0-1.5.75.75 0 0 1 0 1.5Zm8.25-6.75a.75.75 0 1 1 0-1.5.75.75 0 0 1 0 1.5Z"
            />
          </svg>
          <span class="label">{workspaceState.branch}</span>
          {#if workspaceState.isDirty}
            <span class="dirty-dot" title="Uncommitted changes" aria-hidden="true"></span>
          {/if}
        </button>

        {#if workspaceState.aheadBehind && (workspaceState.aheadBehind.ahead > 0 || workspaceState.aheadBehind.behind > 0)}
          {@const ahead = workspaceState.aheadBehind.ahead}
          {@const behind = workspaceState.aheadBehind.behind}
          <button
            class="status-item sync"
            aria-label="{ahead > 0 ? `${ahead} ahead` : ''}{ahead > 0 && behind > 0
              ? ', '
              : ''}{behind > 0 ? `${behind} behind` : ''}"
            title="{ahead > 0 ? `${ahead} ahead` : ''}{ahead > 0 && behind > 0 ? ', ' : ''}{behind >
            0
              ? `${behind} behind`
              : ''}"
          >
            {#if ahead > 0}
              <span class="sync-label" aria-hidden="true">&#8593;{ahead}</span>
            {/if}
            {#if behind > 0}
              <span class="sync-label" aria-hidden="true">&#8595;{behind}</span>
            {/if}
          </button>
        {/if}
      {/if}

      {#if worktreeCount > 1 && worktreeName}
        <span class="status-item worktree" title="Worktree: {worktreeName}">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
            <path
              d="M1.75 1A1.75 1.75 0 0 0 0 2.75v10.5C0 14.216.784 15 1.75 15h12.5A1.75 1.75 0 0 0 16 13.25v-8.5A1.75 1.75 0 0 0 14.25 3H7.5a.25.25 0 0 1-.2-.1l-.9-1.2C6.07 1.26 5.55 1 5 1H1.75Z"
            />
          </svg>
          <span class="label">{worktreeName}</span>
        </span>
      {/if}
    </div>

    <!-- CENTER: focused pane context -->
    <div class="section center">
      {#if focusedPaneKind === 'agent' && activeAgent}
        <span
          class="status-item agent-status"
          class:permission={activeAgent.status.type === 'waitingPermission'}
          class:error={activeAgent.status.type === 'error'}
          class:working={activeAgent.status.type === 'thinking' ||
            activeAgent.status.type === 'toolCalling' ||
            activeAgent.status.type === 'compacting'}
        >
          {agentStatusLabel(activeAgent)}
        </span>

        {#if activeAgent.model}
          <span class="status-item dim">{activeAgent.model}</span>
        {/if}

        {#if activeAgent.contextPercent != null}
          <span
            class="status-item context"
            style="color: {contextColor(activeAgent.contextPercent)}"
            title="Context window usage"
          >
            ctx {activeAgent.contextPercent}%
          </span>
        {/if}

        {#if activeAgent.costUsd != null}
          <span class="status-item dim" title="Session cost">
            {formatCost(activeAgent.costUsd)}
          </span>
        {/if}

        {#if taskProgress}
          <span class="status-item dim" title="Task progress">
            {taskProgress.done}/{taskProgress.total} tasks
          </span>
        {/if}
      {:else if focusedPaneKind === 'sdkChat' && activeSdkSession}
        {#if activeSdkSession.status === 'streaming'}
          <span class="status-item agent-status working">Thinking</span>
        {:else if activeSdkSession.status === 'error'}
          <span class="status-item agent-status error">Error</span>
        {/if}

        {#if activeSdkSession.conversation?.model}
          <span class="status-item dim">{activeSdkSession.conversation.model}</span>
        {/if}

        {#if sdkContextPercent != null}
          <span
            class="status-item context"
            style="color: {contextColor(sdkContextPercent)}"
            title="Context: {contextTokensUsedFor(
              activeSdkSession,
            ).toLocaleString()} / {activeSdkSession.contextWindow?.toLocaleString() ?? '?'} tokens"
          >
            ctx {sdkContextPercent}%
          </span>
        {/if}

        {#if activeSdkSession.costUsd > 0}
          <span class="status-item dim" title="Session cost (estimate)">
            {formatCost(activeSdkSession.costUsd)}
          </span>
        {/if}
      {:else if focusedPaneKind === 'shell'}
        <span class="status-item dim">Shell</span>
      {:else if focusedPaneKind === 'notes'}
        <span class="status-item dim">Notes</span>
      {:else if focusedPaneKind === 'drawing'}
        <span class="status-item dim">Drawing</span>
      {/if}
    </div>

    <!-- RIGHT: global indicators -->
    <div class="section right">
      {#if perfHudEnabled && perfHudState.metrics}
        <button
          class="status-item dim perf-hud"
          aria-label="App CPU {perfHudState.metrics.cpu}%, RAM {perfHudState.metrics.memMb} MB"
          title="App CPU / RAM — click to open settings"
          onclick={openPerfHudSettings}
        >
          {perfHudState.metrics.cpu}% · {perfHudState.metrics.memMb} MB
        </button>
      {/if}

      {#if agentCount > 0}
        <button
          class="status-item agents"
          aria-label="{agentCount} agent{agentCount !== 1 ? 's' : ''}{globalWorstStatus !== 'none'
            ? `, ${globalStatusLabel(globalWorstStatus)}`
            : ''}"
          title="{agentCount} agent{agentCount !== 1 ? 's' : ''}{globalWorstStatus !== 'none'
            ? ` — ${globalStatusLabel(globalWorstStatus)}`
            : ''}"
          onclick={focusWorstAgent}
        >
          <span
            class="agent-dot"
            class:pulse={globalWorstStatus === 'waitingPermission' ||
              globalWorstStatus === 'working'}
            style="background: {statusDotColor(globalWorstStatus)}"
          ></span>
          <span class="label">{agentCount}</span>
        </button>
      {/if}

      {#if permissionSessionId}
        <button
          class="status-item permission-bell"
          aria-label="Agent waiting for permission"
          title="Agent waiting for permission"
          onclick={focusPermissionAgent}
        >
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
            <path
              d="M8 16a2 2 0 0 0 2-2H6a2 2 0 0 0 2 2ZM8 1.918l-.797.161A4.002 4.002 0 0 0 4 6c0 .628-.134 2.197-.459 3.742-.16.767-.376 1.566-.663 2.258h10.244c-.287-.692-.502-1.49-.663-2.258C12.134 8.197 12 6.628 12 6a4.002 4.002 0 0 0-3.203-3.92L8 1.917ZM14.22 12c.223.447.481.801.78 1H1c.299-.199.557-.553.78-1C2.68 10.2 3 6.88 3 6c0-2.42 1.72-4.44 4.005-4.901a1 1 0 1 1 1.99 0A5.002 5.002 0 0 1 13 6c0 .88.32 4.2 1.22 6Z"
            />
          </svg>
        </button>
      {/if}

      {#if hasUpdate}
        <button
          class="status-item update"
          aria-label="Install update v{updateState.version}"
          title="Update v{updateState.version} ready to install"
          onclick={installUpdate}
        >
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
            <path
              d="M8 0a8 8 0 1 1 0 16A8 8 0 0 1 8 0ZM1.5 8a6.5 6.5 0 1 0 13 0 6.5 6.5 0 0 0-13 0Zm4.879-2.773 1.371 1.37V3.5a.25.25 0 0 1 .25-.25h.006a.25.25 0 0 1 .25.25v3.097l1.371-1.37a.25.25 0 0 1 .354.353l-1.793 1.793a.252.252 0 0 1-.166.073h-.027a.252.252 0 0 1-.166-.073L6.025 5.58a.25.25 0 0 1 .354-.353ZM5 10.75a.75.75 0 0 1 .75-.75h4.5a.75.75 0 0 1 0 1.5h-4.5a.75.75 0 0 1-.75-.75Z"
            />
          </svg>
        </button>
      {/if}

      <button
        class="status-item inspector-toggle"
        aria-label="Toggle Inspector"
        title="Toggle Inspector"
        onclick={() => toggleRightPanel()}
      >
        <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
          <path
            d="M8 2c1.981 0 3.671.992 4.933 2.078 1.27 1.091 2.187 2.345 2.637 3.023a1.62 1.62 0 0 1 0 1.798c-.45.678-1.367 1.932-2.637 3.023C11.67 13.008 9.981 14 8 14c-1.981 0-3.671-.992-4.933-2.078C1.797 10.83.88 9.576.43 8.898a1.62 1.62 0 0 1 0-1.798c.45-.677 1.367-1.931 2.637-3.022C4.33 2.992 6.019 2 8 2ZM1.679 7.932a.12.12 0 0 0 0 .136c.411.622 1.241 1.75 2.366 2.717C5.176 11.758 6.527 12.5 8 12.5c1.473 0 2.825-.742 3.955-1.715 1.124-.967 1.954-2.096 2.366-2.717a.12.12 0 0 0 0-.136c-.412-.621-1.242-1.75-2.366-2.717C10.824 4.242 9.473 3.5 8 3.5c-1.473 0-2.824.742-3.955 1.715-1.124.967-1.954 2.096-2.366 2.717ZM8 10a2 2 0 1 1-.001-3.999A2 2 0 0 1 8 10Z"
          />
        </svg>
      </button>

      <button
        class="status-item settings-btn"
        aria-label="Open Settings"
        title="Open Settings"
        onclick={() => showPreferences()}
      >
        <Settings size={13} />
      </button>
    </div>
  </footer>
{/if}

<style>
  .status-bar {
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 8px;
    background: var(--c-bg-glass-heavy);
    border-top: 1px solid var(--c-border-subtle);
    font-size: 11px;
    color: var(--c-text-secondary);
    user-select: none;
    -webkit-app-region: no-drag;
    flex-shrink: 0;
  }

  .section {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .section.left {
    flex: 1;
    justify-content: flex-start;
  }

  .section.center {
    flex: 0 1 auto;
    justify-content: center;
    gap: 6px;
    overflow: hidden;
  }

  .section.right {
    flex: 1;
    justify-content: flex-end;
  }

  .status-item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 5px;
    height: 20px;
    border-radius: 3px;
    white-space: nowrap;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    cursor: default;
    line-height: 1;
  }

  button.status-item {
    cursor: pointer;
  }

  button.status-item:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 140px;
  }

  .branch .label {
    max-width: 160px;
  }

  .dirty-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c-warning-text);
    flex-shrink: 0;
  }

  .sync-label {
    font-variant-numeric: tabular-nums;
  }

  .dim {
    color: var(--c-text-muted);
  }

  .perf-hud {
    font-variant-numeric: tabular-nums;
  }

  /* Agent status in center */
  .agent-status {
    color: var(--c-text-secondary);
    font-weight: 500;
  }

  .agent-status.working {
    color: var(--c-accent-text);
  }

  .agent-status.permission {
    color: var(--c-warning-text);
  }

  .agent-status.error {
    color: var(--c-danger-text);
  }

  .context {
    font-variant-numeric: tabular-nums;
  }

  /* Right section */
  .agent-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .agent-dot.pulse {
    animation: badge-pulse 1.5s ease-in-out infinite;
  }

  .permission-bell {
    color: var(--c-warning-text);
    animation: badge-pulse 1.5s ease-in-out infinite;
  }

  .update {
    color: var(--c-accent-text);
  }

  .inspector-toggle {
    color: var(--c-text-muted);
  }

  .inspector-toggle:hover {
    color: var(--c-text);
  }

  .settings-btn {
    color: var(--c-text-muted);
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
    .agent-dot.pulse,
    .permission-bell {
      animation: none;
    }
  }

  svg {
    flex-shrink: 0;
  }
</style>
