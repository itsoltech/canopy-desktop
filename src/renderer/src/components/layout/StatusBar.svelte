<script lang="ts">
  import { match } from 'ts-pattern'
  import { Settings, GitBranch, Folder, Bell, ArrowDownToLine, Eye, Cpu } from '@lucide/svelte'
  import { workspaceState, projects, toggleRightPanel } from '../../lib/stores/workspace.svelte'
  import { agentSessions, type AgentSessionState } from '../../lib/agents/agentState.svelte'
  import { getAllTabs, activeTabId, focusSessionByPtyId } from '../../lib/stores/tabs.svelte'
  import { findLeaf } from '../../lib/stores/splitTree'
  import { updateState, installUpdate } from '../../lib/stores/updateState.svelte'
  import { prefs } from '../../lib/stores/preferences.svelte'
  import { perfHudState, enablePerfHud, disablePerfHud } from '../../lib/stores/perfHud.svelte'
  import { showPreferences } from '../../lib/stores/dialogs.svelte'

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

  type PaneKind = 'agent' | 'shell' | 'browser' | 'editor' | 'notes' | 'drawing' | 'none'

  let focusedPaneKind: PaneKind = $derived.by(() => {
    if (!focusedPane) return 'none'
    if (focusedPane.paneType === 'browser') return 'browser'
    if (focusedPane.paneType === 'editor') return 'editor'
    if (focusedPane.paneType === 'notes') return 'notes'
    if (focusedPane.paneType === 'drawing') return 'drawing'
    if (AI_TOOL_IDS.has(focusedPane.toolId)) return 'agent'
    return 'shell'
  })

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
      .with('waitingPermission', () => 'var(--color-warning-text)')
      .with('error', () => 'var(--color-danger-text)')
      .with('working', () => 'var(--color-accent-text)')
      .with('idle', () => 'var(--color-success)')
      .otherwise(() => 'var(--color-text-faint)')
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
    if (pct >= 90) return 'var(--color-danger-text)'
    if (pct >= 70) return 'var(--color-warning-text)'
    return 'var(--color-success)'
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
  <footer
    class="flex items-center justify-between gap-4 w-full h-status-bar px-3 pb-px bg-bg-glass border-t border-border-subtle font-sans text-xs font-medium text-text-faint select-none flex-shrink-0 app-no-drag"
  >
    <!-- LEFT: focused-pane info -->
    <div class="flex items-center gap-2 min-w-0">
      {#if focusedPaneKind === 'agent' && activeAgent}
        <span
          class="inline-flex items-center h-5 px-1.5 whitespace-nowrap font-medium"
          class:text-text-secondary={activeAgent.status.type !== 'thinking' &&
            activeAgent.status.type !== 'toolCalling' &&
            activeAgent.status.type !== 'compacting' &&
            activeAgent.status.type !== 'waitingPermission' &&
            activeAgent.status.type !== 'error'}
          class:text-accent-text={activeAgent.status.type === 'thinking' ||
            activeAgent.status.type === 'toolCalling' ||
            activeAgent.status.type === 'compacting'}
          class:text-warning-text={activeAgent.status.type === 'waitingPermission'}
          class:text-danger-text={activeAgent.status.type === 'error'}
        >
          {agentStatusLabel(activeAgent)}
        </span>

        {#if activeAgent.model}
          <span class="inline-flex items-center h-5 whitespace-nowrap text-text-faint"
            >{activeAgent.model}</span
          >
        {/if}

        {#if activeAgent.contextPercent != null}
          <span
            class="inline-flex items-center h-5 whitespace-nowrap font-mono tabular-nums"
            style="color: {contextColor(activeAgent.contextPercent)}"
            title="Context window usage"
          >
            ctx {activeAgent.contextPercent}%
          </span>
        {/if}

        {#if activeAgent.costUsd != null}
          <span
            class="inline-flex items-center h-5 whitespace-nowrap font-mono text-text-faint"
            title="Session cost"
          >
            {formatCost(activeAgent.costUsd)}
          </span>
        {/if}

        {#if taskProgress}
          <span
            class="inline-flex items-center h-5 whitespace-nowrap text-text-faint tabular-nums"
            title="Task progress"
          >
            {taskProgress.done}/{taskProgress.total} tasks
          </span>
        {/if}
      {:else if focusedPaneKind === 'shell'}
        <span class="inline-flex items-center h-5 text-text-faint">Shell</span>
      {:else if focusedPaneKind === 'notes'}
        <span class="inline-flex items-center h-5 text-text-faint">Notes</span>
      {:else if focusedPaneKind === 'drawing'}
        <span class="inline-flex items-center h-5 text-text-faint">Drawing</span>
      {/if}
    </div>

    <!-- RIGHT: three sub-groups separated by hairline dividers -->
    <div class="flex items-center min-w-0">
      <!-- Sub-group A: git -->
      <div class="flex items-center gap-2">
        {#if workspaceState.isGitRepo && workspaceState.branch}
          <button
            class="inline-flex items-center gap-1.5 h-5 px-1.5 rounded-sm border-0 bg-transparent font-inherit text-text-faint cursor-pointer transition-colors duration-fast hover:bg-hover hover:text-text-secondary"
            aria-label="Branch: {workspaceState.branch}{workspaceState.isDirty
              ? ', uncommitted changes'
              : ''}"
            title="Branch: {workspaceState.branch}"
          >
            <GitBranch size={12} class="flex-shrink-0" />
            <span class="overflow-hidden text-ellipsis max-w-40 font-mono text-xs whitespace-nowrap"
              >{workspaceState.branch}</span
            >
            {#if workspaceState.isDirty}
              <span
                class="size-1.5 rounded-full bg-warning-text flex-shrink-0"
                title="Uncommitted changes"
                aria-hidden="true"
              ></span>
            {/if}
          </button>

          {#if workspaceState.aheadBehind && (workspaceState.aheadBehind.ahead > 0 || workspaceState.aheadBehind.behind > 0)}
            {@const ahead = workspaceState.aheadBehind.ahead}
            {@const behind = workspaceState.aheadBehind.behind}
            <button
              class="inline-flex items-center gap-1 h-5 px-1.5 rounded-sm border-0 bg-border-subtle font-inherit text-text-secondary font-mono text-2xs font-semibold tabular-nums leading-none cursor-pointer transition-colors duration-fast hover:bg-hover hover:text-text"
              aria-label="{ahead > 0 ? `${ahead} ahead` : ''}{ahead > 0 && behind > 0
                ? ', '
                : ''}{behind > 0 ? `${behind} behind` : ''}"
              title="{ahead > 0 ? `${ahead} ahead` : ''}{ahead > 0 && behind > 0
                ? ', '
                : ''}{behind > 0 ? `${behind} behind` : ''}"
            >
              {#if ahead > 0}<span aria-hidden="true">↑{ahead}</span>{/if}
              {#if behind > 0}<span aria-hidden="true">↓{behind}</span>{/if}
            </button>
          {/if}
        {/if}

        {#if worktreeCount > 1 && worktreeName}
          <span
            class="inline-flex items-center gap-1.5 h-5 px-1.5 whitespace-nowrap text-text-faint"
            title="Worktree: {worktreeName}"
          >
            <Folder size={12} class="flex-shrink-0" />
            <span class="overflow-hidden text-ellipsis max-w-35">{worktreeName}</span>
          </span>
        {/if}
      </div>

      {#if perfHudEnabled || agentCount > 0 || permissionSessionId || hasUpdate}
        <span class="self-stretch w-px bg-border-subtle mx-3" aria-hidden="true"></span>
      {/if}

      <!-- Sub-group B: perf / agents / alerts -->
      <div class="flex items-center gap-2">
        {#if perfHudEnabled && perfHudState.metrics}
          <button
            class="inline-flex items-center gap-1.5 h-5 px-1.5 rounded-sm border-0 bg-border-subtle font-inherit text-text-secondary tabular-nums leading-none cursor-pointer transition-colors duration-fast hover:bg-hover hover:text-text"
            aria-label="App CPU {perfHudState.metrics.cpu}%, RAM {perfHudState.metrics.memMb} MB"
            title="App CPU / RAM — click to open settings"
            onclick={openPerfHudSettings}
          >
            <Cpu size={11} class="flex-shrink-0 text-text-faint" />
            <span class="text-2xs font-mono" style="transform: translateY(1px)"
              >{perfHudState.metrics.cpu}% · {perfHudState.metrics.memMb} MB</span
            >
          </button>
        {/if}

        {#if agentCount > 0}
          <button
            class="inline-flex items-center gap-1.5 h-5 px-1.5 rounded-sm border-0 bg-transparent font-inherit text-text-faint cursor-pointer transition-colors duration-fast hover:bg-hover hover:text-text-secondary"
            aria-label="{agentCount} agent{agentCount !== 1 ? 's' : ''}{globalWorstStatus !== 'none'
              ? `, ${globalStatusLabel(globalWorstStatus)}`
              : ''}"
            title="{agentCount} agent{agentCount !== 1 ? 's' : ''}{globalWorstStatus !== 'none'
              ? ` — ${globalStatusLabel(globalWorstStatus)}`
              : ''}"
            onclick={focusWorstAgent}
          >
            <span
              class="size-2 rounded-full flex-shrink-0"
              class:animate-badge-pulse={globalWorstStatus === 'waitingPermission' ||
                globalWorstStatus === 'working'}
              class:motion-reduce:animate-none={globalWorstStatus === 'waitingPermission' ||
                globalWorstStatus === 'working'}
              style="background: {statusDotColor(globalWorstStatus)}"
            ></span>
            <span
              class="inline-flex items-center justify-center min-w-4 h-4 px-1 rounded-sm bg-border-subtle text-text-secondary text-2xs font-semibold tabular-nums leading-none"
              >{agentCount}</span
            >
          </button>
        {/if}

        {#if permissionSessionId}
          <button
            class="inline-flex items-center h-5 px-1.5 rounded-sm border-0 bg-transparent font-inherit text-warning-text animate-badge-pulse motion-reduce:animate-none cursor-pointer transition-colors duration-fast hover:bg-hover"
            aria-label="Agent waiting for permission"
            title="Agent waiting for permission"
            onclick={focusPermissionAgent}
          >
            <Bell size={13} class="flex-shrink-0" />
          </button>
        {/if}

        {#if hasUpdate}
          <button
            class="inline-flex items-center h-5 px-1.5 rounded-sm border-0 bg-transparent font-inherit text-accent-text cursor-pointer transition-colors duration-fast hover:bg-hover"
            aria-label="Install update v{updateState.version}"
            title="Update v{updateState.version} ready to install"
            onclick={installUpdate}
          >
            <ArrowDownToLine size={13} class="flex-shrink-0" />
          </button>
        {/if}
      </div>

      <span class="self-stretch w-px bg-border-subtle mx-3" aria-hidden="true"></span>

      <!-- Sub-group C: app actions -->
      <div class="flex items-center gap-1">
        <button
          class="inline-flex items-center justify-center size-5 rounded-sm border-0 bg-transparent font-inherit text-text-faint cursor-pointer transition-colors duration-fast hover:bg-hover hover:text-text-secondary"
          aria-label="Toggle Inspector"
          title="Toggle Inspector"
          onclick={() => toggleRightPanel()}
        >
          <Eye size={13} />
        </button>

        <button
          class="inline-flex items-center justify-center size-5 rounded-sm border-0 bg-transparent font-inherit text-text-faint cursor-pointer transition-colors duration-fast hover:bg-hover hover:text-text-secondary"
          aria-label="Open Settings"
          title="Open Settings"
          onclick={() => showPreferences()}
        >
          <Settings size={13} />
        </button>
      </div>
    </div>
  </footer>
{/if}
