<script lang="ts">
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { Search, X, Loader2, Copy, Filter, Send } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { setPref, getPref, prefs } from '../../lib/stores/preferences.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { getActiveAgentPane } from '../../lib/stores/tabs.svelte'
  import { fetchAndFormatTaskContext } from '../../lib/taskTracker/taskContext'
  import BranchCreateForm from './BranchCreateForm.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  const DONE_STATUS_PATTERN = /^(done|closed|resolved|cancelled|rejected|complete|gotowe|zamkni)/i

  interface Task {
    key: string
    summary: string
    description: string
    status: string
    priority: string
    type: string
    parentKey?: string
    sprintName?: string
    sprintNumber?: number
    assignee?: string
    url?: string
  }

  interface Board {
    id: string
    name: string
    projectKey?: string
  }

  function filterPrefKey(connId: string, boardId: string): string {
    return `taskTracker.pickerFilters.${connId}.${boardId}`
  }

  interface SavedFilters {
    excludedStatuses: string[]
    assignedToMe: boolean
    showFilters: boolean
  }

  function loadSavedFilters(connId: string, boardId: string): SavedFilters | null {
    const raw = prefs[filterPrefKey(connId, boardId)]
    if (!raw) return null
    try {
      return JSON.parse(raw) as SavedFilters
    } catch {
      return null
    }
  }

  function saveFilters(): void {
    if (!selectedBoardId) return
    const data: SavedFilters = {
      excludedStatuses: [...excludedStatuses],
      assignedToMe,
      showFilters,
    }
    setPref(filterPrefKey(connectionId, selectedBoardId), JSON.stringify(data))
  }

  let { connectionId }: { connectionId: string } = $props()

  let allTasks: Task[] = $state([])
  let loading = $state(true)
  let error = $state('')
  let searchQuery = $state('')
  let selectedIndex = $state(0)

  // Board selector
  let boards: Board[] = $state([])
  let selectedBoardId = $state('')

  // Status filter
  let availableStatuses: string[] = $derived.by(() => {
    const seen = new SvelteSet<string>()
    for (const task of allTasks) {
      if (task.status) seen.add(task.status)
    }
    return Array.from(seen).sort()
  })
  let excludedStatuses = new SvelteSet<string>()
  let showFilters = $state(false)
  let assignedToMe = $state(false)
  let currentUserName = $state('')
  let hasSavedFilters = $state(false)

  let selectedBoardProjectKey = $derived.by(() => {
    const board = boards.find((b) => b.id === selectedBoardId)
    return board?.projectKey ?? ''
  })

  let filteredTasks = $derived.by(() => {
    let result = allTasks
    // Filter by board's project key
    if (selectedBoardProjectKey) {
      result = result.filter((i) => i.key.startsWith(selectedBoardProjectKey + '-'))
    }
    if (assignedToMe && currentUserName) {
      result = result.filter((i) => i.assignee === currentUserName)
    }
    if (excludedStatuses.size > 0) {
      result = result.filter((i) => !excludedStatuses.has(i.status))
    }
    if (searchQuery) {
      const q = searchQuery.toLowerCase()
      result = result.filter(
        (i) => i.key.toLowerCase().includes(q) || i.summary.toLowerCase().includes(q),
      )
    }
    return result
  })

  const DISPLAY_LIMIT = 200
  let displayedTasks = $derived(filteredTasks.slice(0, DISPLAY_LIMIT))

  // Branch creation state
  let selectedTask: Task | null = $state(null)

  onMount(async () => {
    await loadBoards()
  })

  async function loadBoards(): Promise<void> {
    try {
      const repoRoot = workspaceState.repoRoot ?? undefined
      const [boardList, userName] = await Promise.all([
        window.api.trackerConfigFetchBoards(repoRoot, connectionId),
        window.api.trackerConfigGetCurrentUser(repoRoot, connectionId).catch(() => ''),
      ])
      boards = boardList
      currentUserName = userName
      if (boards.length > 0) {
        const lastBoard = getPref(`taskTracker.lastBoard.${connectionId}`)
        selectedBoardId = boards.some((b) => b.id === lastBoard) ? lastBoard : boards[0].id
        restoreSavedFilters()
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load boards'
    }
    await fetchTasks()
  }

  function restoreSavedFilters(): void {
    excludedStatuses.clear()
    const saved = loadSavedFilters(connectionId, selectedBoardId)
    if (saved) {
      for (const s of saved.excludedStatuses) excludedStatuses.add(s)
      assignedToMe = saved.assignedToMe
      showFilters = saved.showFilters
      hasSavedFilters = true
    } else {
      assignedToMe = false
      showFilters = false
      hasSavedFilters = false
    }
  }

  async function onBoardChange(): Promise<void> {
    setPref(`taskTracker.lastBoard.${connectionId}`, selectedBoardId)
    restoreSavedFilters()
    await fetchTasks()
  }

  async function fetchTasks(): Promise<void> {
    loading = true
    error = ''
    try {
      allTasks = await window.api.trackerConfigFetchTasks(
        workspaceState.repoRoot ?? undefined,
        connectionId,
        { boardId: selectedBoardId || undefined },
      )
      // Auto-exclude done/closed only if no saved filters
      if (!hasSavedFilters && excludedStatuses.size === 0 && allTasks.length > 0) {
        for (const task of allTasks) {
          if (DONE_STATUS_PATTERN.test(task.status)) {
            excludedStatuses.add(task.status)
          }
        }
        saveFilters()
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to fetch tasks'
    } finally {
      loading = false
    }
  }

  function toggleStatus(status: string): void {
    if (excludedStatuses.has(status)) {
      excludedStatuses.delete(status)
    } else {
      excludedStatuses.add(status)
    }
    saveFilters()
  }

  function toggleAssignedToMe(): void {
    assignedToMe = !assignedToMe
    saveFilters()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      closeDialog()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedIndex = Math.min(selectedIndex + 1, displayedTasks.length - 1)
      scrollToSelected()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      selectedIndex = Math.max(selectedIndex - 1, 0)
      scrollToSelected()
    } else if (e.key === 'Enter' && displayedTasks[selectedIndex]) {
      selectTask(displayedTasks[selectedIndex])
    }
  }

  function scrollToSelected(): void {
    const el = document.querySelector('.task-row.selected')
    el?.scrollIntoView({ block: 'nearest' })
  }

  function selectTask(task: Task): void {
    if (!workspaceState.repoRoot || !workspaceState.branch) return
    selectedTask = $state.snapshot(task) as Task
  }

  function cancelBranchCreation(): void {
    selectedTask = null
  }

  async function copyTaskToClipboard(task: Task, e: MouseEvent): Promise<void> {
    e.stopPropagation()
    const text = `${task.key}: ${task.summary}\n\n${task.description || ''}`
    try {
      await navigator.clipboard.writeText(text.trim())
      addToast('Copied to clipboard')
      closeDialog()
    } catch (err) {
      console.error('Failed to copy task to clipboard', err)
      addToast('Failed to copy to clipboard')
    }
  }

  let hasActiveAgent = $derived(!!getActiveAgentPane())

  async function sendTaskToAgent(task: Task, e: MouseEvent): Promise<void> {
    e.stopPropagation()
    const pane = getActiveAgentPane()
    if (!pane) return
    const context = await fetchAndFormatTaskContext(
      connectionId,
      task,
      workspaceState.repoRoot ?? undefined,
    )
    await window.api.writePty(pane.sessionId, context + '\n')
    addToast('Task sent to agent')
    closeDialog()
  }

  function priorityColor(priority: string): string {
    const p = priority.toLowerCase()
    if (p.includes('critical') || p.includes('highest')) return 'var(--c-danger)'
    if (p.includes('high')) return 'var(--c-warning)'
    if (p.includes('medium') || p.includes('normal')) return 'var(--c-warning-text)'
    if (p.includes('low')) return 'var(--c-accent)'
    return 'var(--c-text-muted)'
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={closeDialog} role="presentation">
  <div
    class="picker-container"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-label="Task Picker"
  >
    {#if selectedTask}
      <BranchCreateForm
        {connectionId}
        {selectedBoardId}
        task={selectedTask}
        onBack={cancelBranchCreation}
      />
    {:else}
      <div class="picker-header">
        <h3 class="picker-title">Select Task</h3>
        <div class="header-actions">
          <button
            class="filter-btn"
            class:active={showFilters}
            onclick={() => {
              showFilters = !showFilters
              saveFilters()
            }}
            title="Filters"
          >
            <Filter size={14} />
          </button>
          <button class="close-btn" onclick={closeDialog} aria-label="Close">
            <X size={16} />
          </button>
        </div>
      </div>

      {#if boards.length > 1}
        <div class="board-row">
          <CustomSelect
            value={selectedBoardId}
            options={boards.map((b) => ({ value: b.id, label: b.name }))}
            onchange={(v) => {
              selectedBoardId = v
              onBoardChange()
            }}
            maxWidth="none"
          />
        </div>
      {/if}

      {#if showFilters}
        <div class="filters-panel">
          <label class="filter-check">
            <CustomCheckbox checked={assignedToMe} onchange={toggleAssignedToMe} />
            <span>Only assigned to me</span>
          </label>
          {#if availableStatuses.length > 0}
            <div class="status-filters">
              {#each availableStatuses as status (status)}
                <button
                  class="status-chip"
                  class:active={!excludedStatuses.has(status)}
                  class:excluded={excludedStatuses.has(status)}
                  onclick={() => toggleStatus(status)}
                >
                  {status}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      <div class="search-row">
        <Search size={14} />
        <input
          class="search-input"
          bind:value={searchQuery}
          placeholder="Search by key or title..."
          oninput={() => (selectedIndex = 0)}
        />
      </div>

      <div class="task-list">
        {#if loading}
          <div class="state-msg">
            <Loader2 size={16} class="spin" />
            <span>Loading tasks...</span>
          </div>
        {:else if error}
          <div class="state-msg error">
            <span>{error}</span>
            <button class="retry-btn" onclick={fetchTasks}>Retry</button>
          </div>
        {:else if displayedTasks.length === 0}
          <div class="state-msg">No tasks found</div>
        {:else}
          {#each displayedTasks as task, i (task.key)}
            <div
              class="task-row"
              class:selected={i === selectedIndex}
              role="button"
              tabindex="0"
              onclick={() => selectTask(task)}
              onkeydown={(e) => {
                if (e.key === 'Enter') selectTask(task)
              }}
              onmouseenter={() => (selectedIndex = i)}
            >
              <span class="task-key">{task.key}</span>
              <span class="task-summary">{task.summary}</span>
              <span class="status-badge">{task.status}</span>
              <span
                class="priority-dot"
                style="color: {priorityColor(task.priority)}"
                title={task.priority}
              >
                ●
              </span>
              {#if hasActiveAgent}
                <button
                  class="send-btn"
                  onclick={(e) => sendTaskToAgent(task, e)}
                  title="Send to agent"
                  aria-label="Send to agent"
                >
                  <Send size={12} />
                </button>
              {/if}
              <button
                class="send-btn"
                onclick={(e) => {
                  e.stopPropagation()
                  copyTaskToClipboard(task, e)
                }}
                title="Copy to clipboard"
              >
                <Copy size={12} />
              </button>
            </div>
          {/each}
        {/if}
      </div>

      <div class="picker-footer">
        <span class="hint">↑↓ navigate · Enter select · Esc close</span>
        <span class="count"
          >{filteredTasks.length > DISPLAY_LIMIT
            ? `${DISPLAY_LIMIT} of ${filteredTasks.length} tasks`
            : `${filteredTasks.length} task${filteredTasks.length !== 1 ? 's' : ''}`}</span
        >
      </div>
    {/if}
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 80px;
    background: var(--c-scrim);
  }

  .picker-container {
    width: 600px;
    max-height: 500px;
    display: flex;
    flex-direction: column;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: 0 16px 48px var(--c-scrim);
    overflow: hidden;
  }

  .picker-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .picker-title {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .filter-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
  }

  .filter-btn:hover {
    background: var(--c-hover);
    color: var(--c-text-secondary);
  }

  .filter-btn.active {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .board-row {
    padding: 6px 16px;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .filters-panel {
    padding: 8px 16px;
    margin: 0;
    border-bottom: 1px solid var(--c-border-subtle);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .filter-check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--c-text-secondary);
    cursor: pointer;
  }

  .status-filters {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .status-chip {
    padding: 2px 8px;
    border: 1px solid var(--c-border);
    border-radius: 12px;
    background: none;
    color: var(--c-text-muted);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .status-chip:hover {
    border-color: var(--c-border);
    color: var(--c-text-secondary);
  }

  .status-chip.active {
    background: var(--c-accent-bg);
    border-color: var(--c-accent-muted);
    color: var(--c-accent-text);
  }

  .status-chip.excluded {
    opacity: 0.4;
    text-decoration: line-through;
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
  }

  .close-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .search-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--c-border-subtle);
    color: var(--c-text-muted);
  }

  .search-input {
    flex: 1;
    border: none;
    background: none;
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .search-input::placeholder {
    color: var(--c-text-faint);
  }

  .task-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .state-msg {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px 16px;
    font-size: 13px;
    color: var(--c-text-muted);
  }

  .state-msg.error {
    color: var(--c-danger-text);
    flex-direction: column;
    gap: 12px;
  }

  .retry-btn {
    padding: 4px 12px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: none;
    color: var(--c-text-secondary);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .retry-btn:hover {
    background: var(--c-hover);
  }

  .task-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 16px;
    border: none;
    background: none;
    color: var(--c-text-secondary);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.05s;
  }

  .task-row:hover,
  .task-row.selected {
    background: var(--c-hover);
  }

  .task-key {
    flex-shrink: 0;
    font-weight: 600;
    color: var(--c-accent-text);
    min-width: 80px;
  }

  .task-summary {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-badge {
    flex-shrink: 0;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--c-active);
    font-size: 10px;
    color: var(--c-text-muted);
  }

  .priority-dot {
    flex-shrink: 0;
    font-size: 8px;
    line-height: 1;
  }

  .send-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--c-text-faint);
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 0.1s;
  }

  .task-row:hover .send-btn,
  .task-row.selected .send-btn {
    opacity: 1;
  }

  .send-btn:hover {
    background: var(--c-hover-strong);
    color: var(--c-generate);
  }

  .picker-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .hint {
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .count {
    font-size: 11px;
    color: var(--c-text-muted);
  }

  :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
