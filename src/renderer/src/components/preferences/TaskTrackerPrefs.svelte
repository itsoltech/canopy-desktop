<script lang="ts">
  import { onMount } from 'svelte'
  import { RefreshCw, Trash2 } from '@lucide/svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import {
    loadRepoConfig,
    getRepoConfig,
    getHasCredentials,
    saveRepoConfig,
    initRepoConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { confirm as confirmDialog } from '../../lib/stores/dialogs.svelte'
  import TaskConnectionsPrefs from './TaskConnectionsPrefs.svelte'
  import TaskBranchNamingPrefs from './TaskBranchNamingPrefs.svelte'
  import TaskPRNamingPrefs from './TaskPRNamingPrefs.svelte'

  let config = $derived(getRepoConfig())
  let hasCreds = $derived(getHasCredentials())
  let repoRoot = $derived(workspaceState.repoRoot)

  let projectKeys = $derived(config ? Object.keys(config.projects) : [])
  let selectedProject = $state<string | null>(null)

  let availableProjects = $state<Array<{ key: string; name: string }>>([])
  let loadingProjects = $state(false)
  let boards = $state<Array<{ id: string; name: string }>>([])
  let placeholders = $state<Array<{ key: string; description: string; example: string }>>([])

  let branchNamingRef: ReturnType<typeof TaskBranchNamingPrefs> | undefined = $state()

  let availableStatuses = $state<string[]>([])
  let loadingStatuses = $state(false)

  onMount(async () => {
    if (!repoRoot) return
    await loadRepoConfig(repoRoot)
    if (projectKeys.length > 0) {
      selectedProject = projectKeys[0]
    }
    if (hasCreds) {
      await fetchAvailableProjects()
    }
    try {
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders({})
    } catch {
      // use empty
    }
    if (config && selectedProject && config.projects[selectedProject]) {
      branchNamingRef?.initTemplate(config.projects[selectedProject].branchTemplate.template)
    }
  })

  async function fetchAvailableProjects(): Promise<void> {
    loadingProjects = true
    try {
      const connections = await window.api.taskTrackerGetConnections()
      if (connections.length > 0) {
        const allBoards = await window.api.taskTrackerFetchBoards(connections[0].id)
        const projectMap: Record<string, string> = {}
        for (const board of allBoards) {
          if (board.projectKey && !projectMap[board.projectKey]) {
            projectMap[board.projectKey] = board.name
          }
        }
        availableProjects = Object.entries(projectMap).map(([key, name]) => ({
          key,
          name: `${key} — ${name}`,
        }))
      }
    } catch {
      // keep empty
    } finally {
      loadingProjects = false
    }
  }

  async function handleInit(): Promise<void> {
    if (!repoRoot) return
    try {
      await initRepoConfig(repoRoot)
      addToast('Tracker configuration initialized')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to initialize config')
    }
  }

  async function addProjectByKey(key: string): Promise<void> {
    if (!config || !repoRoot || !key) return
    if (config.projects[key]) {
      selectedProject = key
      return
    }
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.projects[key] = {
      branchTemplate: {
        template: '{branchType}/{taskKey}-{taskTitle}',
        customVars: {},
      },
      prTemplate: {
        titleTemplate: '[{taskKey}] {taskTitle}',
        bodyTemplate: '## {taskKey}: {taskTitle}\n\n{taskUrl}',
        defaultTargetBranch: '',
        targetRules: [],
      },
      boardOverrides: {},
    }
    await saveRepoConfig(repoRoot, updated)
    selectedProject = key
    branchNamingRef?.initTemplate(updated.projects[key].branchTemplate.template)
    addToast(`Project ${key} added`)
  }

  async function removeProject(key: string): Promise<void> {
    if (!config || !repoRoot) return
    const ok = await confirmDialog({
      title: 'Remove Project',
      message: `Remove project ${key} and its naming configuration?`,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    delete updated.projects[key]
    await saveRepoConfig(repoRoot, updated)
    if (selectedProject === key) {
      const keys = Object.keys(updated.projects)
      selectedProject = keys.length > 0 ? keys[0] : null
    }
    addToast(`Project ${key} removed`)
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
      const vars =
        selectedProject && config?.projects[selectedProject]
          ? config.projects[selectedProject].branchTemplate.customVars
          : {}
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

    <div class="subsection">
      <h4 class="subsection-title">Projects</h4>

      {#if projectKeys.length > 0}
        <div class="select-row">
          <span class="select-label">Active project</span>
          <CustomSelect
            value={selectedProject ?? ''}
            options={projectKeys.map((k) => ({ value: k, label: k }))}
            onchange={(v) => {
              selectedProject = v
              if (config?.projects[v]) {
                branchNamingRef?.initTemplate(config.projects[v].branchTemplate.template)
              }
            }}
            maxWidth="180px"
          />
          {#if selectedProject}
            <button
              class="icon-btn destructive"
              onclick={() => selectedProject && removeProject(selectedProject)}
              title="Remove project"
            >
              <Trash2 size={14} />
            </button>
          {/if}
        </div>
      {/if}

      {#if availableProjects.length > 0}
        {@const unaddedProjects = availableProjects.filter((p) => !config?.projects[p.key])}
        {#if unaddedProjects.length > 0}
          <div class="select-row">
            <span class="select-label">Add project</span>
            <CustomSelect
              value=""
              options={[
                {
                  value: '',
                  label: loadingProjects ? 'Loading...' : 'Select project...',
                },
                ...unaddedProjects.map((p) => ({ value: p.key, label: p.name })),
              ]}
              onchange={(v) => {
                if (v) addProjectByKey(v)
              }}
              maxWidth="240px"
            />
            <button
              class="icon-btn"
              onclick={fetchAvailableProjects}
              disabled={loadingProjects}
              title="Refresh projects"
            >
              <RefreshCw size={12} />
            </button>
          </div>
        {/if}
      {:else if hasCreds}
        <div>
          <button
            class="btn btn-add-item"
            onclick={fetchAvailableProjects}
            disabled={loadingProjects}
          >
            {#if loadingProjects}Loading...{:else}+ Load projects from tracker{/if}
          </button>
        </div>
      {:else}
        <span class="hint-text">Add credentials above to load projects from your tracker.</span>
      {/if}
    </div>

    {#if selectedProject && config.projects[selectedProject]}
      <TaskBranchNamingPrefs
        bind:this={branchNamingRef}
        {repoRoot}
        projectKey={selectedProject}
        {boards}
        {placeholders}
        onTemplateChanged={refreshPlaceholders}
      />

      <TaskPRNamingPrefs {repoRoot} projectKey={selectedProject} {boards} />
    {/if}

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

  .select-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .select-label {
    color: var(--c-text-secondary);
    min-width: 110px;
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

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .btn-primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--c-accent-bg-hover);
  }

  .btn-add-item {
    align-self: flex-start;
    padding: 6px 14px;
    border: 1px dashed var(--c-text-faint);
    border-radius: 6px;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-add-item:hover:not(:disabled) {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .btn-add-item:disabled {
    opacity: 0.5;
    cursor: default;
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

  .icon-btn.destructive:hover {
    color: var(--c-danger-text);
  }
</style>
