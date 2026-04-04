<script lang="ts">
  import { onMount } from 'svelte'
  import { RefreshCw, Plus, Trash2 } from '@lucide/svelte'
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
  let newProjectKey = $state('')

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
    try {
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders({})
    } catch {
      // use empty
    }
    if (config && selectedProject && config.projects[selectedProject]) {
      branchNamingRef?.initTemplate(config.projects[selectedProject].branchTemplate.template)
    }
  })

  async function handleInit(): Promise<void> {
    if (!repoRoot) return
    try {
      await initRepoConfig(repoRoot)
      addToast('Tracker configuration initialized')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to initialize config')
    }
  }

  async function addProject(): Promise<void> {
    if (!config || !repoRoot || !newProjectKey.trim()) return
    const key = newProjectKey.trim().toUpperCase()
    if (config.projects[key]) {
      addToast(`Project ${key} already exists`)
      return
    }
    const updated = structuredClone($state.snapshot(config)) as typeof config
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
    newProjectKey = ''
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
    const updated = structuredClone($state.snapshot(config)) as typeof config
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
    const updated = structuredClone($state.snapshot(config)) as typeof config
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
    const updated = structuredClone($state.snapshot(config)) as typeof config
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
    <p class="section-desc">
      No <code>.canopy/config.json</code> found in this repository.
    </p>
    <button class="btn btn-primary" onclick={handleInit}>Initialize Configuration</button>
  </div>
{:else}
  <TaskConnectionsPrefs {repoRoot} />

  <div class="section">
    <h3 class="section-title">Projects</h3>
    <p class="section-desc">Configure naming per project. A tracker can have multiple projects.</p>

    {#if projectKeys.length > 0}
      <div class="form-row">
        <label class="form-label">Project</label>
        <CustomSelect
          value={selectedProject ?? ''}
          options={projectKeys.map((k) => ({ value: k, label: k }))}
          onchange={(v) => {
            selectedProject = v
            if (config?.projects[v]) {
              branchNamingRef?.initTemplate(config.projects[v].branchTemplate.template)
            }
          }}
          maxWidth="none"
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

    <div class="inline-form">
      <input
        class="form-input"
        bind:value={newProjectKey}
        placeholder="Project key (e.g. GAKKO)"
        onkeydown={(e) => {
          if (e.key === 'Enter') addProject()
        }}
      />
      <button
        class="icon-btn"
        onclick={addProject}
        disabled={!newProjectKey.trim()}
        title="Add project"
      >
        <Plus size={14} />
      </button>
    </div>
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

  <div class="section">
    <h3 class="section-title">Task Filters</h3>
    <p class="section-desc">Configure which tasks to fetch from the tracker.</p>

    <label class="checkbox-row">
      <CustomCheckbox checked={config.filters.assignedToMe} onchange={toggleAssignedToMe} />
      <span>Only show tasks assigned to me</span>
    </label>

    <h4 class="subsection-title" style="margin-top: 12px;">
      Status Filter
      <button
        class="icon-btn"
        onclick={loadStatusesFromApi}
        disabled={loadingStatuses}
        title="Refresh from API"
      >
        <RefreshCw size={12} />
      </button>
    </h4>
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
      <p class="hint-text">Click refresh to load statuses from your tracker.</p>
    {/if}
  </div>
{/if}

<style>
  .section {
    margin-bottom: 24px;
  }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0 0 4px;
  }

  .section-desc {
    font-size: 12px;
    color: var(--c-text-muted);
    margin: 0 0 12px;
  }

  .subsection-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--c-text-secondary);
    margin: 0 0 8px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .form-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .form-label {
    font-size: 12px;
    color: var(--c-text-secondary);
    width: 90px;
    flex-shrink: 0;
  }

  .form-input {
    flex: 1;
    padding: 5px 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
  }

  .form-input:focus {
    border-color: var(--c-focus-ring);
  }

  .inline-form {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border: none;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.1s;
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
    opacity: 0.4;
    cursor: default;
  }

  .icon-btn.destructive:hover {
    color: var(--c-danger-text);
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    font-size: 13px;
    color: var(--c-text-secondary);
    cursor: pointer;
  }

  .hint-text {
    font-size: 12px;
    color: var(--c-text-faint);
    margin: 4px 0;
  }
</style>
