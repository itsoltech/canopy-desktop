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
    // Auto-init global config if it doesn't exist (read store directly to avoid stale $derived)
    if (!getGlobalConfig()) {
      await initGlobalConfig()
    }

    if (repoRoot) {
      await loadRepoConfig(repoRoot)
      // Default to project scope when repo has config
      if (repoConfig) scope = 'project'
    }

    if (hasCreds) {
      await fetchBoards()
    }

    if (config?.branchTemplate) {
      branchNamingRef?.initTemplate(config.branchTemplate?.template ?? '')
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

<div class="section">
  <h3 class="section-title">Task Tracker</h3>

  <div class="scope-tabs">
    <button
      class="scope-tab"
      class:active={scope === 'global'}
      onclick={() => handleScopeChange('global')}
    >
      Global
    </button>
    <button
      class="scope-tab"
      class:active={scope === 'project'}
      disabled={!repoRoot}
      onclick={() => handleScopeChange('project')}
      title={repoRoot
        ? 'Project-specific settings (.canopy/config.json)'
        : 'Open a repository to configure project settings'}
    >
      Project
    </button>
  </div>

  <span class="scope-hint">
    {#if scope === 'global'}
      Your personal settings, used across all projects.
    {:else}
      Stored in <code>.canopy/config.json</code> — shared with your team via git. Overrides global settings.
    {/if}
  </span>

  {#if scope === 'project' && !repoRoot}
    <p class="hint-text">Open a repository to configure project settings.</p>
  {:else if scope === 'project' && !repoConfig}
    <div>
      <span class="hint-text">
        No <code>.canopy/config.json</code> found in this repository.
      </span>
      <div style="margin-top: 8px">
        <button class="btn btn-primary" onclick={handleInitProject}
          >Initialize Project Config</button
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

    <div class="subsection">
      <h4 class="subsection-title">Filters</h4>

      <label class="checkbox-row">
        <CustomCheckbox checked={config.filters.assignedToMe} onchange={toggleAssignedToMe} />
        <span>Only show tasks assigned to me</span>
      </label>

      <div class="filter-header">
        <span class="filter-label">Status filter</span>
        <button
          class="icon-btn"
          onclick={loadStatusesFromApi}
          disabled={loadingStatuses}
          title="Refresh from API"
        >
          <RefreshCw size={12} />
        </button>
      </div>
      {#if availableStatuses.length > 0}
        {#each availableStatuses as status (status)}
          <label class="checkbox-row">
            <CustomCheckbox
              checked={config.filters.statuses.includes(status)}
              onchange={() => toggleStatus(status)}
            />
            <span>{status}</span>
          </label>
        {/each}
      {:else}
        <span class="hint-text">Click refresh to load statuses from your tracker.</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .scope-tabs {
    display: flex;
    gap: 2px;
    background: var(--c-border-subtle);
    border-radius: 8px;
    padding: 2px;
    width: fit-content;
  }

  .scope-tab {
    padding: 5px 16px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--c-text-muted);
    font-size: 12px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    transition: all 0.15s;
  }

  .scope-tab:hover:not(:disabled) {
    color: var(--c-text-secondary);
  }

  .scope-tab.active {
    background: var(--c-bg);
    color: var(--c-text);
    box-shadow: 0 1px 2px var(--c-shadow, rgba(0, 0, 0, 0.1));
  }

  .scope-tab:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .scope-hint {
    font-size: 11px;
    color: var(--c-text-faint);
    margin-top: -12px;
  }

  .subsection {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .subsection-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--c-text-muted);
    margin: 0;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
  }

  .filter-header {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .filter-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--c-text-secondary);
  }

  .hint-text {
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .btn {
    padding: 6px 14px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-primary:hover {
    background: var(--c-accent-bg-hover);
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--c-hover);
    color: var(--c-text-secondary);
  }

  .icon-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
