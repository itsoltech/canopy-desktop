<script lang="ts">
  import { onMount } from 'svelte'
  import { RefreshCw } from '@lucide/svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import {
    loadRepoConfig,
    getRepoConfig,
    getHasCredentials,
    saveRepoConfig,
    initRepoConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import TaskConnectionsPrefs from './TaskConnectionsPrefs.svelte'
  import TaskBranchNamingPrefs from './TaskBranchNamingPrefs.svelte'
  import TaskPRNamingPrefs from './TaskPRNamingPrefs.svelte'

  let config = $derived(getRepoConfig())
  let hasCreds = $derived(getHasCredentials())
  let repoRoot = $derived(workspaceState.repoRoot)

  let boards = $state<Array<{ id: string; name: string }>>([])
  let placeholders = $state<Array<{ key: string; description: string; example: string }>>([])

  let branchNamingRef: ReturnType<typeof TaskBranchNamingPrefs> | undefined = $state()

  let availableStatuses = $state<string[]>([])
  let loadingStatuses = $state(false)

  onMount(async () => {
    if (!repoRoot) return
    await loadRepoConfig(repoRoot)
    if (hasCreds) {
      await fetchBoards()
    }
    try {
      const vars = config?.branchTemplate?.customVars ?? {}
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders(vars)
    } catch {
      // use empty
    }
    if (config) {
      branchNamingRef?.initTemplate(config.branchTemplate.template)
    }
  })

  async function fetchBoards(): Promise<void> {
    try {
      const connections = await window.api.taskTrackerGetConnections()
      if (connections.length > 0) {
        boards = await window.api.taskTrackerFetchBoards(connections[0].id)
      }
    } catch {
      boards = []
    }
  }

  async function handleInit(): Promise<void> {
    if (!repoRoot) return
    try {
      const newConfig = await initRepoConfig(repoRoot)
      branchNamingRef?.initTemplate(newConfig.branchTemplate.template)
      addToast('Tracker configuration initialized')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to initialize config')
    }
  }

  async function loadStatusesFromApi(): Promise<void> {
    if (!config || !hasCreds) return
    loadingStatuses = true
    try {
      const connections = await window.api.taskTrackerGetConnections()
      if (connections.length > 0) {
        const statuses = await window.api.taskTrackerFetchStatuses(connections[0].id)
        availableStatuses = statuses.map((s) => s.name)
      }
    } catch {
      addToast('Failed to fetch statuses')
    } finally {
      loadingStatuses = false
    }
  }

  async function toggleAssignedToMe(): Promise<void> {
    if (!config || !repoRoot) return
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.filters = { ...updated.filters, assignedToMe: !updated.filters.assignedToMe }
    await saveRepoConfig(repoRoot, updated)
  }

  async function toggleStatus(status: string): Promise<void> {
    if (!config || !repoRoot) return
    const current = [...config.filters.statuses]
    const idx = current.indexOf(status)
    if (idx >= 0) {
      current.splice(idx, 1)
    } else {
      current.push(status)
    }
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.filters = { ...updated.filters, statuses: current }
    await saveRepoConfig(repoRoot, updated)
  }

  async function refreshPlaceholders(): Promise<void> {
    try {
      const vars = config?.branchTemplate?.customVars ?? {}
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders(vars)
    } catch {
      // keep current
    }
  }
</script>

{#if !repoRoot}
  <p class="hint-text">Open a repository to configure task tracker.</p>
{:else if !config}
  <div class="section">
    <h3 class="section-title">Task Tracker</h3>
    <span class="hint-text">
      No <code>.canopy/config.json</code> found in this repository.
    </span>
    <div>
      <button class="btn btn-primary" onclick={handleInit}>Initialize Configuration</button>
    </div>
  </div>
{:else}
  <div class="section">
    <h3 class="section-title">Task Tracker</h3>

    <TaskConnectionsPrefs {repoRoot} />

    <TaskBranchNamingPrefs
      bind:this={branchNamingRef}
      {repoRoot}
      {boards}
      {placeholders}
      onTemplateChanged={refreshPlaceholders}
    />

    <TaskPRNamingPrefs {repoRoot} {boards} />

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
  </div>
{/if}

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
