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
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

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

<div class="flex flex-col gap-7">
  <div class="flex flex-col gap-2">
    <div
      class="inline-flex w-fit p-0.5 bg-bg-input border border-border-subtle rounded-md select-none"
      role="group"
      aria-label="Settings scope"
    >
      <button
        type="button"
        aria-pressed={scope === 'global'}
        class="px-3 py-1 border-0 rounded-sm text-sm font-inherit cursor-pointer disabled:opacity-40 disabled:cursor-default"
        class:bg-bg={scope === 'global'}
        class:text-text={scope === 'global'}
        class:bg-transparent={scope !== 'global'}
        class:text-text-muted={scope !== 'global'}
        class:hover:text-text-secondary={scope !== 'global'}
        onclick={() => handleScopeChange('global')}
      >
        Global
      </button>
      <button
        type="button"
        aria-pressed={scope === 'project'}
        class="px-3 py-1 border-0 rounded-sm text-sm font-inherit cursor-pointer disabled:opacity-40 disabled:cursor-default"
        class:bg-bg={scope === 'project'}
        class:text-text={scope === 'project'}
        class:bg-transparent={scope !== 'project'}
        class:text-text-muted={scope !== 'project'}
        class:hover:text-text-secondary={scope !== 'project'}
        disabled={!repoRoot}
        onclick={() => handleScopeChange('project')}
        title={repoRoot
          ? 'Project-specific settings (.canopy/config.json)'
          : 'Open a repository to configure project settings'}
      >
        Project
      </button>
    </div>

    <p class="text-xs text-text-muted m-0 leading-snug">
      {#if scope === 'global'}
        Personal settings used across all projects.
      {:else}
        Stored in <code class="font-mono text-text-secondary">.canopy/config.json</code> — shared with
        your team via git. Overrides global settings.
      {/if}
    </p>
  </div>

  {#if scope === 'project' && !repoRoot}
    <p class="text-sm text-text-faint m-0">Open a repository to configure project settings.</p>
  {:else if scope === 'project' && !repoConfig}
    <div class="flex flex-col gap-2 items-start">
      <p class="text-sm text-text-faint m-0">
        No <code class="font-mono text-text-secondary">.canopy/config.json</code> in this repository.
      </p>
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
        onclick={handleInitProject}>Initialize project config</button
      >
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

    <PrefsSection title="Filters" description="Limit which tasks appear in pickers">
      <PrefsRow
        label="Only show tasks assigned to me"
        help="Hides tasks assigned to other people or unassigned"
        search="filter assigned to me mine"
      >
        <CustomCheckbox checked={config.filters.assignedToMe} onchange={toggleAssignedToMe} />
      </PrefsRow>

      <PrefsRow
        label="Status filter"
        help={availableStatuses.length === 0
          ? 'Click refresh to load statuses from your tracker.'
          : `${config.filters.statuses.length} of ${availableStatuses.length} selected`}
        search="filter status statuses"
        layout="stacked"
      >
        <div class="flex flex-col gap-2">
          <button
            type="button"
            class="self-start flex items-center gap-1 px-2.5 py-1 rounded-md bg-transparent border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text disabled:opacity-50 disabled:cursor-default"
            onclick={loadStatusesFromApi}
            disabled={loadingStatuses}
            title="Refresh from API"
          >
            <RefreshCw size={12} />
            <span>{loadingStatuses ? 'Loading…' : 'Refresh from API'}</span>
          </button>
          {#if availableStatuses.length > 0}
            <div class="flex flex-wrap gap-x-4 gap-y-1.5">
              {#each availableStatuses as status (status)}
                <label class="flex items-center gap-2 text-md text-text cursor-pointer">
                  <CustomCheckbox
                    checked={config.filters.statuses.includes(status)}
                    onchange={() => toggleStatus(status)}
                  />
                  <span>{status}</span>
                </label>
              {/each}
            </div>
          {/if}
        </div>
      </PrefsRow>
    </PrefsSection>
  {/if}
</div>
