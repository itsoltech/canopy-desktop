<script lang="ts">
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { Search, X, LoaderCircle, Copy, Funnel, Send } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { setPref, getPref } from '../../lib/stores/preferences.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { getActiveAgentPane, switchTab } from '../../lib/stores/tabs.svelte'
  import {
    logTaskToAgentFailure,
    noActiveAgentOutcome,
    sendTaskToAgentContext,
    tabFocusFailedOutcome,
    taskToAgentUserMessage,
  } from '../../lib/taskTracker/taskToAgent'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import { statusChipClass } from '../../lib/taskTracker/statusChip'
  import { unlockSizeOnResize } from '../../lib/actions/resizableDialog'
  import BranchCreateForm from './BranchCreateForm.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  import {
    DONE_STATUS_PATTERN,
    NO_SPRINT,
    loadSavedTaskFilters,
    saveTaskFilters,
    taskDisplayKey,
  } from '../../lib/taskTracker/taskFilterPrefs'

  interface Task {
    key: string
    summary: string
    description: string
    status: string
    statusCategory?: string
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

  function saveFilters(): void {
    saveTaskFilters(connectionId, selectedBoardId, {
      excludedStatuses: [...excludedStatuses],
      excludedSprints: [...excludedSprints],
      assignedToMe,
    })
  }

  let { connectionId }: { connectionId: string } = $props()

  let allTasks: Task[] = $state([])
  let loading = $state(true)
  let error = $state('')
  let searchQuery = $state('')
  let selectedIndex = $state(0)

  let boards: Board[] = $state([])
  let selectedBoardId = $state('')

  let availableStatuses: string[] = $derived.by(() => {
    const seen = new SvelteSet<string>()
    for (const task of allTasks) {
      if (task.status) seen.add(task.status)
    }
    return Array.from(seen).sort()
  })
  let availableSprints: string[] = $derived.by(() => {
    const seen = new SvelteSet<string>()
    let hasNoSprint = false
    for (const task of allTasks) {
      if (task.sprintName) seen.add(task.sprintName)
      else hasNoSprint = true
    }
    const sorted = Array.from(seen).sort()
    return hasNoSprint ? [...sorted, NO_SPRINT] : sorted
  })
  let excludedStatuses = new SvelteSet<string>()
  let excludedSprints = new SvelteSet<string>()
  // Filters start expanded — hiding them made the default status/assignee filtering invisible.
  let showFilters = $state(true)
  let assignedToMe = $state(false)
  let currentUserName = $state('')
  let hasSavedFilters = $state(false)

  let selectedBoardProjectKey = $derived.by(() => {
    const board = boards.find((b) => b.id === selectedBoardId)
    return board?.projectKey ?? ''
  })

  let filteredTasks = $derived.by(() => {
    let result = allTasks
    if (selectedBoardProjectKey) {
      result = result.filter((i) => i.key.startsWith(selectedBoardProjectKey + '-'))
    }
    if (assignedToMe && currentUserName) {
      result = result.filter((i) => i.assignee === currentUserName)
    }
    if (excludedStatuses.size > 0) {
      result = result.filter((i) => !excludedStatuses.has(i.status))
    }
    if (excludedSprints.size > 0) {
      result = result.filter((i) => !excludedSprints.has(i.sprintName || NO_SPRINT))
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

  let selectedTask: Task | null = $state(null)

  let searchInputEl: HTMLInputElement | null = $state(null)
  let dialogEl: HTMLDivElement | undefined = $state()
  let sendingTaskKey = $state('')
  let sendStatus = $state('')
  let sendError = $state('')

  onMount(async () => {
    searchInputEl?.focus()
    await loadBoards()
  })

  // Tracker config (.canopy/config.json) lives in the ACTIVE WORKTREE — same path the Project
  // tracker modal edits. The main repo root may hold a stale copy from the default branch.
  let cfgRoot = $derived(
    workspaceState.selectedWorktreePath ?? workspaceState.repoRoot ?? undefined,
  )

  async function loadBoards(): Promise<void> {
    try {
      const repoRoot = cfgRoot
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
      error = ipcErrorMessage(e, 'Failed to load boards')
    }
    await fetchTasks()
  }

  function restoreSavedFilters(): void {
    excludedStatuses.clear()
    excludedSprints.clear()
    const saved = loadSavedTaskFilters(connectionId, selectedBoardId)
    if (saved) {
      for (const s of saved.excludedStatuses) excludedStatuses.add(s)
      for (const s of saved.excludedSprints ?? []) excludedSprints.add(s)
      assignedToMe = saved.assignedToMe
      hasSavedFilters = true
    } else {
      assignedToMe = false
      hasSavedFilters = false
    }
  }

  async function onBoardChange(): Promise<void> {
    clearTaskSendFeedback()
    setPref(`taskTracker.lastBoard.${connectionId}`, selectedBoardId)
    restoreSavedFilters()
    await fetchTasks()
  }

  async function fetchTasks(): Promise<void> {
    loading = true
    error = ''
    try {
      allTasks = await window.api.trackerConfigFetchTasks(cfgRoot, connectionId, {
        boardId: selectedBoardId || undefined,
      })
      if (!hasSavedFilters && excludedStatuses.size === 0 && allTasks.length > 0) {
        for (const task of allTasks) {
          if (DONE_STATUS_PATTERN.test(task.status)) {
            excludedStatuses.add(task.status)
          }
        }
        saveFilters()
      }
    } catch (e) {
      error = ipcErrorMessage(e, 'Failed to fetch tasks')
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

  function toggleSprint(sprint: string): void {
    if (excludedSprints.has(sprint)) {
      excludedSprints.delete(sprint)
    } else {
      excludedSprints.add(sprint)
    }
    saveFilters()
  }

  function toggleAssignedToMe(): void {
    assignedToMe = !assignedToMe
    saveFilters()
  }

  function handleKeydown(e: KeyboardEvent): void {
    // When the branch-create sub-view is open, this window-level handler must
    // not drive the (hidden) task list. Escape returns to the list; every other
    // key is left to the sub-view's own inputs.
    if (selectedTask) {
      if (e.key === 'Escape') {
        e.preventDefault()
        cancelBranchCreation()
      }
      return
    }
    if (e.key === 'Tab' && dialogEl) {
      const focusable = dialogEl.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement as HTMLElement | null
      if (e.shiftKey && (active === first || !dialogEl.contains(active))) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
      return
    }
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
    const el = document.querySelector('[data-task-selected="true"]')
    el?.scrollIntoView({ block: 'nearest' })
  }

  function selectTask(task: Task): void {
    if (!workspaceState.repoRoot || !workspaceState.branch) return
    clearTaskSendFeedback()
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

  function clearTaskSendFeedback(): void {
    if (sendingTaskKey) return
    sendStatus = ''
    sendError = ''
  }

  async function sendTaskToAgent(task: Task, e: MouseEvent): Promise<void> {
    e.stopPropagation()
    if (sendingTaskKey) return

    sendingTaskKey = task.key
    sendStatus = `Sending ${task.key} to agent...`
    sendError = ''

    const result = getActiveAgentPane()
    if (!result) {
      const outcome = noActiveAgentOutcome()
      sendStatus = ''
      sendError = taskToAgentUserMessage(outcome)
      sendingTaskKey = ''
      logTaskToAgentFailure(outcome, { taskKey: task.key, connectionId })
      return
    }

    try {
      await switchTab(result.tabId)
    } catch (error) {
      const outcome = tabFocusFailedOutcome(error, result.pane.sessionId)
      sendStatus = ''
      sendError = taskToAgentUserMessage(outcome)
      sendingTaskKey = ''
      logTaskToAgentFailure(outcome, {
        taskKey: task.key,
        connectionId,
        sessionId: result.pane.sessionId,
      })
      return
    }

    const outcome = await sendTaskToAgentContext({
      connectionId,
      task: $state.snapshot(task) as Task,
      repoRoot: workspaceState.repoRoot ?? undefined,
      target: {
        worktreePath: workspaceState.selectedWorktreePath ?? undefined,
        sessionId: result.pane.sessionId,
      },
    })
    sendingTaskKey = ''
    sendStatus = ''
    if (outcome.status !== 'sent') {
      sendError = taskToAgentUserMessage(outcome)
      logTaskToAgentFailure(outcome, {
        taskKey: task.key,
        connectionId,
        sessionId: result.pane.sessionId,
      })
      return
    }

    addToast('Task sent to agent')
    closeDialog()
  }

  function priorityColor(priority: string): string {
    const p = priority.toLowerCase()
    if (p.includes('critical') || p.includes('highest')) return 'var(--color-danger)'
    if (p.includes('high')) return 'var(--color-warning)'
    if (p.includes('medium') || p.includes('normal')) return 'var(--color-warning-text)'
    if (p.includes('low')) return 'var(--color-accent)'
    return 'var(--color-text-muted)'
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="fixed inset-0 z-[1001] flex justify-center items-start pt-20 bg-scrim"
  onclick={closeDialog}
  role="presentation"
>
  <div
    bind:this={dialogEl}
    class="resize w-[600px] min-w-[480px] max-w-[94vw] min-h-[200px] max-h-[500px] flex flex-col bg-bg-overlay border border-border rounded-[10px] shadow-[0_16px_48px_var(--color-scrim)] overflow-hidden"
    use:unlockSizeOnResize
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
      <div
        class="flex items-center justify-between px-4 pt-3.5 pb-2.5 border-b border-border-subtle"
      >
        <h3 class="m-0 text-lg font-semibold text-text">Select Task</h3>
        <div class="flex items-center gap-1">
          <button
            class="flex items-center justify-center w-7 h-7 border-0 rounded-md bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text-secondary"
            class:!bg-accent-bg={showFilters}
            class:!text-accent-text={showFilters}
            onclick={() => {
              showFilters = !showFilters
              saveFilters()
            }}
            title="Filters"
            aria-label="Toggle filters"
          >
            <Funnel size={14} />
          </button>
          <button
            class="flex items-center justify-center w-7 h-7 border-0 rounded-md bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text"
            onclick={closeDialog}
            aria-label="Close"
          >
            <X size={16} />
          </button>
        </div>
      </div>

      {#if boards.length > 1}
        <div class="px-4 py-1.5 border-b border-border-subtle">
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
        <div class="px-4 py-2 border-b border-border-subtle flex flex-col gap-2">
          <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
            <CustomCheckbox checked={assignedToMe} onchange={toggleAssignedToMe} />
            <span>Only tasks assigned to me</span>
          </label>
          {#if availableStatuses.length > 0}
            <div class="flex flex-col gap-1">
              <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
                >Status</span
              >
              <div class="flex flex-wrap gap-1">
                {#each availableStatuses as status (status)}
                  <button
                    class="px-2 py-0.5 border border-border rounded-xl bg-transparent text-text-muted text-xs font-inherit cursor-pointer transition-colors duration-fast hover:text-text-secondary"
                    class:!bg-accent-bg={!excludedStatuses.has(status)}
                    class:!border-accent-muted={!excludedStatuses.has(status)}
                    class:!text-accent-text={!excludedStatuses.has(status)}
                    class:!opacity-40={excludedStatuses.has(status)}
                    class:line-through={excludedStatuses.has(status)}
                    onclick={() => toggleStatus(status)}
                  >
                    {status}
                  </button>
                {/each}
              </div>
            </div>
          {/if}
          {#if availableSprints.length > 0}
            <div class="flex flex-col gap-1">
              <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
                >Sprint</span
              >
              <div class="flex flex-wrap gap-1">
                {#each availableSprints as sprint (sprint)}
                  <button
                    class="px-2 py-0.5 border border-border rounded-xl bg-transparent text-text-muted text-xs font-inherit cursor-pointer transition-colors duration-fast hover:text-text-secondary"
                    class:!bg-accent-bg={!excludedSprints.has(sprint)}
                    class:!border-accent-muted={!excludedSprints.has(sprint)}
                    class:!text-accent-text={!excludedSprints.has(sprint)}
                    class:!opacity-40={excludedSprints.has(sprint)}
                    class:line-through={excludedSprints.has(sprint)}
                    onclick={() => toggleSprint(sprint)}
                  >
                    {sprint}
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {/if}

      <div class="flex items-center gap-2 px-4 py-2 border-b border-border-subtle text-text-muted">
        <Search size={14} />
        <input
          class="flex-1 border-0 bg-transparent text-text text-md font-inherit outline-none placeholder:text-text-faint"
          bind:this={searchInputEl}
          bind:value={searchQuery}
          aria-label="Search tasks"
          placeholder="Search by key or title..."
          oninput={() => {
            selectedIndex = 0
            clearTaskSendFeedback()
          }}
        />
      </div>

      <span class="sr-only" role="status" aria-live="polite">{sendError || sendStatus}</span>
      {#if sendStatus || sendError}
        <div
          class="flex items-start gap-2 px-4 py-2 border-b border-border-subtle text-xs leading-snug"
          class:bg-danger-bg={sendError}
          class:text-danger-text={sendError}
          class:bg-bg-input={!sendError}
          class:text-text-muted={!sendError}
        >
          <span class="flex-1">{sendError || sendStatus}</span>
          {#if sendError}
            <button
              class="flex items-center justify-center w-5 h-5 -mr-1 border-0 rounded bg-transparent text-current cursor-pointer opacity-70 hover:opacity-100 hover:bg-hover"
              onclick={clearTaskSendFeedback}
              aria-label="Dismiss task send error"
            >
              <X size={12} />
            </button>
          {/if}
        </div>
      {/if}

      <div class="flex-1 overflow-y-auto py-1">
        {#if loading}
          <div class="flex items-center justify-center gap-2 px-4 py-6 text-md text-text-muted">
            <LoaderCircle size={16} class="animate-spin motion-reduce:animate-none" />
            <span>Loading tasks...</span>
          </div>
        {:else if error}
          <div
            class="flex flex-col items-center justify-center gap-3 px-4 py-6 text-md text-danger-text"
          >
            <span>{error}</span>
            <button
              class="px-3 py-1 border border-border rounded-lg bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover"
              onclick={fetchTasks}>Retry</button
            >
          </div>
        {:else if displayedTasks.length === 0}
          <div class="flex items-center justify-center gap-2 px-4 py-6 text-md text-text-muted">
            No tasks found
          </div>
        {:else}
          {#each displayedTasks as task, i (task.key)}
            <div
              class="flex items-center gap-2 w-full px-4 py-1.5 border-0 bg-transparent text-text-secondary text-sm font-inherit cursor-pointer text-left transition-colors duration-fast group/task hover:bg-hover"
              class:!bg-hover={i === selectedIndex}
              data-task-selected={i === selectedIndex}
              role="button"
              tabindex="0"
              onclick={() => selectTask(task)}
              onkeydown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  selectTask(task)
                }
              }}
              onmouseenter={() => (selectedIndex = i)}
            >
              <span class="flex-shrink-0 font-semibold text-accent-text min-w-20"
                >{taskDisplayKey(task)}</span
              >
              <span
                class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap"
                title={task.summary}>{task.summary}</span
              >
              {#if task.sprintName}
                <span
                  class="flex-shrink-0 max-w-24 overflow-hidden text-ellipsis whitespace-nowrap px-1.5 py-px rounded-md border border-border-subtle text-2xs text-text-faint"
                  title={`Sprint: ${task.sprintName}`}>{task.sprintName}</span
                >
              {/if}
              {#if task.assignee}
                <span
                  class="flex-shrink-0 max-w-28 overflow-hidden text-ellipsis whitespace-nowrap px-1.5 py-px rounded-md bg-active text-2xs text-text-muted"
                  class:!text-accent-text={task.assignee === currentUserName}
                  title={`Assignee: ${task.assignee}`}>{task.assignee}</span
                >
              {/if}
              <span
                class="flex-shrink-0 px-1.5 py-px rounded-md text-2xs {statusChipClass(
                  task.statusCategory,
                )}">{task.status}</span
              >
              <span
                class="flex-shrink-0 text-[8px] leading-none"
                style="color: {priorityColor(task.priority)}"
                role="img"
                aria-label="Priority: {task.priority}"
                title={task.priority}>●</span
              >
              {#if hasActiveAgent}
                <button
                  class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-faint cursor-pointer flex-shrink-0 opacity-0 transition-opacity duration-fast group-hover/task:opacity-100 hover:bg-hover-strong hover:text-generate"
                  onclick={(e) => sendTaskToAgent(task, e)}
                  disabled={Boolean(sendingTaskKey)}
                  title="Send to agent"
                  aria-label="Send to agent"
                >
                  {#if sendingTaskKey === task.key}
                    <LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" />
                  {:else}
                    <Send size={12} />
                  {/if}
                </button>
              {/if}
              <button
                class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-faint cursor-pointer flex-shrink-0 opacity-0 transition-opacity duration-fast group-hover/task:opacity-100 hover:bg-hover-strong hover:text-generate"
                onclick={(e) => {
                  e.stopPropagation()
                  copyTaskToClipboard(task, e)
                }}
                title="Copy to clipboard"
                aria-label="Copy task to clipboard"
              >
                <Copy size={12} />
              </button>
            </div>
          {/each}
        {/if}
      </div>

      <div class="flex items-center justify-between px-4 py-2 border-t border-border-subtle">
        <span class="text-xs text-text-faint">↑↓ navigate · Enter select · Esc close</span>
        <span class="text-xs text-text-muted"
          >{filteredTasks.length > DISPLAY_LIMIT
            ? `${DISPLAY_LIMIT} of ${filteredTasks.length} tasks`
            : `${filteredTasks.length} task${filteredTasks.length !== 1 ? 's' : ''}`}</span
        >
      </div>
    {/if}
  </div>
</div>
