<script lang="ts">
  import { onMount } from 'svelte'
  import { RefreshCw } from '@lucide/svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import {
    loadRepoConfig,
    loadGlobalConfig,
    getRepoConfig,
    getGlobalConfig,
    hasAnyCredentials,
    saveRepoConfig,
    saveGlobalConfig,
    initRepoConfig,
    initGlobalConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import TaskConnectionsPrefs from './TaskConnectionsPrefs.svelte'
  import TaskBranchNamingPrefs from './TaskBranchNamingPrefs.svelte'
  import TaskPRNamingPrefs from './TaskPRNamingPrefs.svelte'

  type ConfigScope = 'global' | 'project'
  let scope = $state<ConfigScope>('global')

  let repoConfig = $derived(getRepoConfig())
  let globalConfig = $derived(getGlobalConfig())
  let config = $derived(scope === 'global' ? globalConfig : repoConfig)
  let hasCreds = $derived(hasAnyCredentials())
  let repoRoot = $derived(workspaceState.repoRoot)

  let boards = $state<Array<{ id: string; name: string }>>([])
  let placeholders = $state<Array<{ key: string; description: string; example: string }>>([])

  let branchNamingRef: ReturnType<typeof TaskBranchNamingPrefs> | undefined = $state()

  let availableStatuses = $state<string[]>([])
  let loadingStatuses = $state(false)

  onMount(async () => {
    try {
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders({})
    } catch {
      // use empty
    }

    await loadGlobalConfig()
    if (!getGlobalConfig()) {
      await initGlobalConfig()
    }

    if (repoRoot) {
      await loadRepoConfig(repoRoot)
      if (getRepoConfig()) scope = 'project'
    }

    if (hasAnyCredentials()) {
      await fetchBoards()
    }

    const currentConfig = scope === 'global' ? getGlobalConfig() : getRepoConfig()
    if (currentConfig?.branchTemplate) {
      branchNamingRef?.initTemplate(currentConfig.branchTemplate?.template ?? '')
    }
  })

  async function fetchBoards(): Promise<void> {
    try {
      boards = await window.api.trackerConfigFetchBoards(repoRoot ?? undefined)
    } catch {
      boards = []
    }
  }

  async function handleInitProject(): Promise<void> {
    if (!repoRoot) return
    try {
      const newConfig = await initRepoConfig(repoRoot)
      branchNamingRef?.initTemplate(newConfig.branchTemplate?.template ?? '')
      addToast('Project tracker configuration initialized')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to initialize config')
    }
  }

  async function loadStatusesFromApi(): Promise<void> {
    if (!config || !hasCreds) return
    loadingStatuses = true
    try {
      const statuses = await window.api.trackerConfigFetchStatuses(repoRoot ?? undefined)
      availableStatuses = statuses.map((s: { name: string }) => s.name)
    } catch {
      addToast('Failed to fetch statuses')
    } finally {
      loadingStatuses = false
    }
  }

  async function toggleAssignedToMe(): Promise<void> {
    if (!config) return
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.filters = { ...updated.filters, assignedToMe: !updated.filters.assignedToMe }
    if (scope === 'global') {
      await saveGlobalConfig(updated)
    } else if (repoRoot) {
      await saveRepoConfig(repoRoot, updated)
    }
  }

  async function toggleStatus(status: string): Promise<void> {
    if (!config) return
    const current = [...config.filters.statuses]
    const idx = current.indexOf(status)
    if (idx >= 0) {
      current.splice(idx, 1)
    } else {
      current.push(status)
    }
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.filters = { ...updated.filters, statuses: current }
    if (scope === 'global') {
      await saveGlobalConfig(updated)
    } else if (repoRoot) {
      await saveRepoConfig(repoRoot, updated)
    }
  }

  async function refreshPlaceholders(): Promise<void> {
    try {
      const vars = config?.branchTemplate?.customVars ?? {}
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders(vars)
    } catch {
      // keep current
    }
  }

  function handleScopeChange(newScope: ConfigScope): void {
    scope = newScope
    if (config?.branchTemplate) {
      branchNamingRef?.initTemplate(config.branchTemplate?.template ?? '')
    }
  }
</script>

<div class="flex flex-col gap-5">
  <h3 class="text-[15px] font-semibold text-text m-0">Task Tracker</h3>

  <div class="flex gap-0.5 bg-border-subtle rounded-xl p-0.5 w-fit">
    <button
      class="px-4 py-1.5 border-0 rounded-lg text-sm font-medium font-inherit cursor-pointer transition-all duration-base disabled:opacity-40 disabled:cursor-default {scope ===
      'global'
        ? '!bg-bg !text-text shadow-[0_1px_2px_oklch(0_0_0/0.1)]'
        : 'bg-transparent text-text-muted enabled:hover:text-text-secondary'}"
      onclick={() => handleScopeChange('global')}
    >
      Global
    </button>
    <button
      class="px-4 py-1.5 border-0 rounded-lg text-sm font-medium font-inherit cursor-pointer transition-all duration-base disabled:opacity-40 disabled:cursor-default {scope ===
      'project'
        ? '!bg-bg !text-text shadow-[0_1px_2px_oklch(0_0_0/0.1)]'
        : 'bg-transparent text-text-muted enabled:hover:text-text-secondary'}"
      disabled={!repoRoot}
      onclick={() => handleScopeChange('project')}
      title={repoRoot
        ? 'Project-specific settings (.canopy/config.json)'
        : 'Open a repository to configure project settings'}
    >
      Project
    </button>
  </div>

  <span class="text-xs text-text-faint -mt-3">
    {#if scope === 'global'}
      Your personal settings, used across all projects.
    {:else}
      Stored in <code>.canopy/config.json</code> — shared with your team via git. Overrides global settings.
    {/if}
  </span>

  {#if scope === 'project' && !repoRoot}
    <p class="text-xs text-text-faint">Open a repository to configure project settings.</p>
  {:else if scope === 'project' && !repoConfig}
    <div>
      <span class="text-xs text-text-faint">
        No <code>.canopy/config.json</code> found in this repository.
      </span>
      <div class="mt-2">
        <button
          class="px-3.5 py-1.5 border-0 rounded-lg text-md font-inherit cursor-pointer bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
          onclick={handleInitProject}>Initialize Project Config</button
        >
      </div>
    </div>
  {:else if config}
    <TaskConnectionsPrefs {repoRoot} {scope} />

    <TaskBranchNamingPrefs
      bind:this={branchNamingRef}
      {repoRoot}
      {boards}
      {placeholders}
      onTemplateChanged={refreshPlaceholders}
      {scope}
    />

    <TaskPRNamingPrefs {repoRoot} {boards} {scope} />

    <div class="flex flex-col gap-2.5">
      <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">Filters</h4>

      <label class="flex items-center gap-2 text-md text-text cursor-pointer">
        <CustomCheckbox checked={config.filters.assignedToMe} onchange={toggleAssignedToMe} />
        <span>Only show tasks assigned to me</span>
      </label>

      <div class="flex items-center gap-1.5">
        <span class="text-sm font-medium text-text-secondary">Status filter</span>
        <button
          class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-muted cursor-pointer enabled:hover:bg-hover enabled:hover:text-text-secondary disabled:opacity-50 disabled:cursor-default"
          onclick={loadStatusesFromApi}
          disabled={loadingStatuses}
          title="Refresh from API"
        >
          <RefreshCw size={12} />
        </button>
      </div>
      {#if availableStatuses.length > 0}
        {#each availableStatuses as status (status)}
          <label class="flex items-center gap-2 text-md text-text cursor-pointer">
            <CustomCheckbox
              checked={config.filters.statuses.includes(status)}
              onchange={() => toggleStatus(status)}
            />
            <span>{status}</span>
          </label>
        {/each}
      {:else}
        <span class="text-xs text-text-faint"
          >Click refresh to load statuses from your tracker.</span
        >
      {/if}
    </div>
  {/if}
</div>
