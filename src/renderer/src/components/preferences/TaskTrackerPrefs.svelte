<script lang="ts">
  import { onMount } from 'svelte'
  import { RefreshCw, Download, Upload } from '@lucide/svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { loadConnections, getTaskTrackerConnections } from '../../lib/stores/taskTracker.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import TaskConnectionsPrefs from './TaskConnectionsPrefs.svelte'
  import TaskBranchNamingPrefs from './TaskBranchNamingPrefs.svelte'
  import TaskPRNamingPrefs from './TaskPRNamingPrefs.svelte'

  let connections = $derived(getTaskTrackerConnections())
  let scopeBoards = $state<Record<string, Array<{ id: string; name: string }>>>({})
  let placeholders = $state<Array<{ key: string; description: string; example: string }>>([])

  let branchNamingRef: ReturnType<typeof TaskBranchNamingPrefs> | undefined = $state()

  async function loadBoardsForConnection(connId: string): Promise<void> {
    if (scopeBoards[connId]) return
    try {
      const boards = await window.api.taskTrackerFetchBoards(connId)
      scopeBoards = { ...scopeBoards, [connId]: boards }
    } catch {
      scopeBoards = { ...scopeBoards, [connId]: [] }
    }
  }

  // Reload boards when connections change
  $effect(() => {
    const ids = connections.map((c) => c.id)
    for (const id of ids) {
      loadBoardsForConnection(id)
    }
  })

  // --- Filters ---
  let assignedToMe = $derived(prefs['taskTracker.assignedToMe'] !== 'false')
  let filterStatuses = $derived.by(() => {
    const raw = prefs['taskTracker.filterStatuses']
    if (!raw) return [] as string[]
    try {
      return JSON.parse(raw) as string[]
    } catch {
      return [] as string[]
    }
  })
  let availableStatuses = $state<string[]>([])
  let loadingStatuses = $state(false)

  // Read branch template from prefs for export
  let branchTemplate = $derived.by(() => {
    const raw = prefs['taskTracker.branchTemplate']
    const fallback = { template: '', customVars: {} as Record<string, string> }
    if (!raw) return fallback
    try {
      return JSON.parse(raw) as { template: string; customVars: Record<string, string> }
    } catch {
      return fallback
    }
  })

  // Read PR config from prefs for export
  let prConfig = $derived.by(() => {
    const raw = prefs['taskTracker.pr']
    if (raw) {
      try {
        const c = JSON.parse(raw) as Record<string, string>
        return {
          titleTemplate: c.titleTemplate || '[{taskKey}] {taskTitle}',
          bodyTemplate: c.bodyTemplate || '## {taskKey}: {taskTitle}\n\n{taskUrl}',
          defaultBranch: c.defaultBranch || 'develop',
        }
      } catch {
        // fall through
      }
    }
    return {
      titleTemplate: prefs['taskTracker.prTitleTemplate'] || '[{taskKey}] {taskTitle}',
      bodyTemplate: prefs['taskTracker.prBodyTemplate'] || '## {taskKey}: {taskTitle}\n\n{taskUrl}',
      defaultBranch: prefs['taskTracker.prDefaultBranch'] || 'develop',
    }
  })

  onMount(async () => {
    await loadConnections()
    await Promise.all(connections.map((c) => loadBoardsForConnection(c.id)))
    try {
      const vars = $state.snapshot(branchTemplate.customVars) as Record<string, string>
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders(vars)
    } catch {
      // use empty
    }
    branchNamingRef?.initTemplate(branchTemplate.template)
  })

  async function loadStatusesFromApi(): Promise<void> {
    if (connections.length === 0) return
    loadingStatuses = true
    try {
      const statuses = await window.api.taskTrackerFetchStatuses(connections[0].id)
      availableStatuses = statuses.map((s) => s.name)
    } catch {
      addToast('Failed to fetch statuses')
    } finally {
      loadingStatuses = false
    }
  }

  function toggleStatus(status: string): void {
    const current = [...filterStatuses]
    const idx = current.indexOf(status)
    if (idx >= 0) {
      current.splice(idx, 1)
    } else {
      current.push(status)
    }
    setPref('taskTracker.filterStatuses', JSON.stringify(current))
  }

  async function refreshPlaceholders(): Promise<void> {
    try {
      const vars = $state.snapshot(branchTemplate.customVars) as Record<string, string>
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders(vars)
    } catch {
      // keep current
    }
  }

  async function exportConfig(): Promise<void> {
    const config = {
      version: 1,
      exportedAt: new Date().toISOString(),
      connections: connections.map((c) => ({
        provider: c.provider,
        name: c.name,
        baseUrl: c.baseUrl,
        projectKey: c.projectKey,
        boardId: c.boardId,
        username: c.username,
      })),
      branchTemplate: branchTemplate,
      prTemplate: {
        titleTemplate: prConfig.titleTemplate,
        bodyTemplate: prConfig.bodyTemplate,
        defaultTargetBranch: prConfig.defaultBranch,
      },
      filters: {
        assignedToMe,
        statuses: filterStatuses,
      },
    }
    const blob = new Blob([JSON.stringify(config, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = 'canopy-task-tracker-config.json'
    a.click()
    URL.revokeObjectURL(url)
    addToast('Configuration exported')
  }

  async function importConfig(): Promise<void> {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.json'
    input.onchange = async () => {
      const file = input.files?.[0]
      if (!file) return
      try {
        const text = await file.text()
        const config = JSON.parse(text)
        if (config.branchTemplate) {
          setPref('taskTracker.branchTemplate', JSON.stringify(config.branchTemplate))
          branchNamingRef?.initTemplate(config.branchTemplate.template || '')
        }
        if (config.prTemplate) {
          setPref(
            'taskTracker.pr',
            JSON.stringify({
              titleTemplate: config.prTemplate.titleTemplate || '',
              bodyTemplate: config.prTemplate.bodyTemplate || '',
              defaultBranch: config.prTemplate.defaultTargetBranch || 'develop',
            }),
          )
        }
        if (config.filters) {
          if (config.filters.assignedToMe !== undefined)
            setPref('taskTracker.assignedToMe', String(config.filters.assignedToMe))
          if (config.filters.statuses)
            setPref('taskTracker.filterStatuses', JSON.stringify(config.filters.statuses))
        }
        addToast('Configuration imported')
      } catch {
        addToast('Invalid configuration file')
      }
    }
    input.click()
  }
</script>

<TaskConnectionsPrefs />

<TaskBranchNamingPrefs
  bind:this={branchNamingRef}
  {connections}
  {scopeBoards}
  {placeholders}
  onTemplateChanged={refreshPlaceholders}
/>

<TaskPRNamingPrefs {connections} {scopeBoards} />

<div class="section">
  <h3 class="section-title">Task Filters</h3>
  <p class="section-desc">Configure which tasks to fetch from the tracker.</p>

  <label
    class="checkbox-row"
    onclick={() => setPref('taskTracker.assignedToMe', assignedToMe ? 'false' : 'true')}
  >
    <CustomCheckbox
      checked={assignedToMe}
      onchange={() => setPref('taskTracker.assignedToMe', assignedToMe ? 'false' : 'true')}
    />
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
      <label class="checkbox-row" onclick={() => toggleStatus(status)}>
        <CustomCheckbox
          checked={filterStatuses.includes(status)}
          onchange={() => toggleStatus(status)}
        />
        <span>{status}</span>
      </label>
    {/each}
  {:else}
    <p class="hint-text">Click refresh to load statuses from your tracker.</p>
  {/if}
</div>

<div class="section">
  <h3 class="section-title">Export / Import</h3>
  <p class="section-desc">Share configuration with your team (credentials are never exported).</p>

  <div class="form-actions">
    <button class="btn btn-secondary" onclick={exportConfig}>
      <Download size={14} /> Export
    </button>
    <button class="btn btn-secondary" onclick={importConfig}>
      <Upload size={14} /> Import
    </button>
  </div>
</div>

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

  .form-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
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

  .btn-secondary {
    background: var(--c-active);
    color: var(--c-text-secondary);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--c-hover-strong);
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
