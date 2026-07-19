<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { X, LoaderCircle, Copy, Send, Link2, Unlink } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import {
    addActiveTask,
    getActiveTasks,
    getResolvedConfig,
    removeActiveTask,
    resolvePanelTask,
  } from '../../lib/stores/taskTracker.svelte'
  import { extractTaskKeys } from '../../lib/taskTracker/branchTaskKey'
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
  import { taskDisplayKey } from '../../lib/taskTracker/taskFilterPrefs'
  import { unlockSizeOnResize } from '../../lib/actions/resizableDialog'
  import type { TrackerProviderKind, TrackerTaskLite } from '../../lib/taskTracker/types'
  import BranchCreateForm from './BranchCreateForm.svelte'
  import TaskListPicker from './TaskListPicker.svelte'
  import NewTaskForm from './NewTaskForm.svelte'

  let { connectionId, mode = 'browse' }: { connectionId: string; mode?: 'browse' | 'link' } =
    $props()

  // Browse mode: a picked task opens the branch-create sub-view.
  let selectedTask: TrackerTaskLite | null = $state(null)
  // Link mode: a picked task waits on the confirmation card until Link is pressed.
  let selectedLinkTask: TrackerTaskLite | null = $state(null)
  // Link mode tabs: pick an existing task or create a new one (auto-linked on creation).
  let linkTab = $state<'existing' | 'newTask'>('existing')

  let dialogEl: HTMLDivElement | undefined = $state()
  let sendingTaskKey = $state('')
  let sendStatus = $state('')
  let sendError = $state('')
  let filteredCount = $state(0)
  const DISPLAY_LIMIT = 200

  // Tracker config (.canopy/config.json) lives in the ACTIVE WORKTREE — same path the Project
  // tracker modal edits. The main repo root may hold a stale copy from the default branch.
  let cfgRoot = $derived(
    workspaceState.selectedWorktreePath ?? workspaceState.repoRoot ?? undefined,
  )

  let provider = $derived(
    (getResolvedConfig()?.config.trackers.find((t) => t.id === connectionId)?.provider ??
      'jira') as TrackerProviderKind,
  )

  let showsTaskList = $derived(mode === 'browse' || (linkTab === 'existing' && !selectedLinkTask))

  function handleKeydown(e: KeyboardEvent): void {
    // When the branch-create sub-view is open, this window-level handler must not interfere.
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
      if (selectedLinkTask) {
        selectedLinkTask = null
      } else {
        closeDialog()
      }
    } else if (e.key === 'Enter' && selectedLinkTask && !linking) {
      // The list's own inputs handle Enter before it bubbles here — this fires on the card.
      e.preventDefault()
      void confirmLink()
    }
  }

  function onPickTask(task: TrackerTaskLite): void {
    clearTaskSendFeedback()
    if (mode === 'link') {
      if (linkedKeys.has(task.key) || branchTaskKeys.has(task.key)) return
      selectedLinkTask = task
      return
    }
    if (!workspaceState.repoRoot || !workspaceState.branch) return
    selectedTask = task
  }

  let linking = $state(false)
  let linkedKeys = $derived(new SvelteSet(getActiveTasks().map((t) => t.taskKey)))
  // Keys embedded in the branch name are tracked by the branch itself — same rule as the sidebar
  // tiles: they read as linked and cannot be unlinked.
  let branchTaskKeys = $derived(
    new SvelteSet(workspaceState.branch ? extractTaskKeys(workspaceState.branch) : []),
  )

  async function unlinkTask(task: TrackerTaskLite): Promise<void> {
    const worktreePath = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    if (!worktreePath || linking || branchTaskKeys.has(task.key)) return
    linking = true
    try {
      await removeActiveTask(worktreePath, task.key)
      await resolvePanelTask(worktreePath, workspaceState.branch)
    } catch (e) {
      addToast(ipcErrorMessage(e, 'Failed to unlink task'))
      return
    } finally {
      linking = false
    }
    addToast(`${task.key} unlinked from this worktree`)
  }

  // Attach the task to the CURRENT worktree (persisted linked task) — no branch is created.
  // On success the dialog closes and the Task panel opens on the linked task.
  async function linkTask(task: TrackerTaskLite): Promise<void> {
    const worktreePath = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    if (!worktreePath || linking || linkedKeys.has(task.key)) return
    linking = true
    try {
      await addActiveTask(worktreePath, {
        taskKey: task.key,
        summary: task.summary,
        connectionId,
      })
      await resolvePanelTask(worktreePath, workspaceState.branch)
    } catch (e) {
      addToast(ipcErrorMessage(e, 'Failed to link task'))
      return
    } finally {
      linking = false
    }
    addToast(`${task.key} linked to this worktree`)
    workspaceState.rightPanelOpen = true
    workspaceState.rightPanelTab = 'task'
    closeDialog()
  }

  async function confirmLink(): Promise<void> {
    if (!selectedLinkTask) return
    await linkTask($state.snapshot(selectedLinkTask) as TrackerTaskLite)
  }

  // New task created in link mode: auto-link it — the creation succeeded, so the only
  // remaining intent is the link itself.
  async function handleCreatedForLink(task: TrackerTaskLite, warnings: string[]): Promise<void> {
    for (const w of warnings) addToast(w)
    await linkTask(task)
  }

  function cancelBranchCreation(): void {
    selectedTask = null
  }

  async function copyTaskToClipboard(task: TrackerTaskLite, e: MouseEvent): Promise<void> {
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

  async function sendTaskToAgent(task: TrackerTaskLite, e: MouseEvent): Promise<void> {
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
      task: $state.snapshot(task) as TrackerTaskLite,
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
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Close on mousedown (not click): a resize-handle drag that ends outside the dialog synthesizes
     a click on this overlay (common ancestor of mousedown/mouseup) and would close it. -->
<div
  class="fixed inset-0 z-[1001] flex justify-center items-start pt-20 bg-scrim"
  onmousedown={closeDialog}
  role="presentation"
>
  <div
    bind:this={dialogEl}
    class="resize w-[600px] min-w-[480px] max-w-[94vw] min-h-[200px] max-h-[500px] flex flex-col bg-bg-overlay border border-border rounded-[10px] shadow-[0_16px_48px_var(--color-scrim)] overflow-hidden"
    use:unlockSizeOnResize
    onmousedown={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-label="Task Picker"
  >
    {#if selectedTask}
      <BranchCreateForm
        {connectionId}
        selectedBoardId=""
        task={selectedTask}
        onBack={cancelBranchCreation}
      />
    {:else}
      <div
        class="flex items-center justify-between px-4 pt-3.5 pb-2.5 border-b border-border-subtle"
      >
        <h3 class="m-0 text-lg font-semibold text-text">
          {mode === 'link' ? 'Link task to this worktree' : 'Select Task'}
        </h3>
        <button
          class="flex items-center justify-center w-7 h-7 border-0 rounded-md bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text"
          onclick={closeDialog}
          aria-label="Close"
        >
          <X size={16} />
        </button>
      </div>

      <span class="sr-only" role="status" aria-live="polite">{sendError || sendStatus}</span>

      <div class="flex-1 min-h-0 px-3 py-3 flex flex-col gap-2">
        {#if mode === 'link'}
          <!-- Same tab styling as the Create Worktree mode switch. -->
          <div
            class="flex gap-0.5 p-0.5 bg-active rounded-lg shrink-0"
            role="group"
            aria-label="Link source"
          >
            <button
              class="flex-1 px-2 py-[5px] border-0 rounded-md text-sm font-inherit cursor-pointer transition-all duration-fast {linkTab ===
              'existing'
                ? '!bg-bg-overlay !text-text shadow-[0_1px_2px_var(--color-scrim)]'
                : 'bg-transparent text-text-muted hover:text-text-secondary'}"
              onclick={() => {
                linkTab = 'existing'
                selectedLinkTask = null
              }}
              aria-pressed={linkTab === 'existing'}
              type="button"
            >
              Existing tasks
            </button>
            <div class="w-px my-1 bg-border shrink-0" aria-hidden="true"></div>
            <button
              class="flex-1 px-2 py-[5px] border-0 rounded-md text-sm font-inherit cursor-pointer transition-all duration-fast {linkTab ===
              'newTask'
                ? '!bg-bg-overlay !text-text shadow-[0_1px_2px_var(--color-scrim)]'
                : 'bg-transparent text-text-muted hover:text-text-secondary'}"
              onclick={() => {
                linkTab = 'newTask'
                selectedLinkTask = null
              }}
              aria-pressed={linkTab === 'newTask'}
              type="button"
              title="Create a task in the tracker — it links to this worktree right away"
            >
              New task
            </button>
          </div>
        {/if}

        {#if mode === 'link' && linkTab === 'newTask'}
          <NewTaskForm
            trackerId={connectionId}
            repoRoot={cfgRoot}
            {provider}
            onCreated={handleCreatedForLink}
            onCancel={closeDialog}
            submitLabel="Create and link task"
          />
        {:else if mode === 'link' && selectedLinkTask}
          <p
            class="m-0 text-md text-text-secondary cursor-help"
            title="This task will be linked to the current worktree — the Task panel tracks it (status, comments)"
          >
            Selected task
          </p>
          <div class="px-3 py-2.5 bg-bg-input border border-border rounded-xl">
            <div class="flex items-center gap-2">
              <span class="font-semibold text-sm text-accent-text"
                >{taskDisplayKey(selectedLinkTask)}</span
              >
              {#if selectedLinkTask.status}
                <span
                  class="text-2xs px-1.5 py-px rounded-md {statusChipClass(
                    selectedLinkTask.statusCategory,
                  )}">{selectedLinkTask.status}</span
                >
              {/if}
              <span class="flex-1"></span>
              <button
                class="flex items-center justify-center size-5 border-0 rounded-md bg-transparent text-text-muted text-sm leading-none cursor-pointer p-0 hover:bg-hover hover:text-text"
                onclick={() => (selectedLinkTask = null)}
                title="Choose a different task"
                aria-label="Change task">×</button
              >
            </div>
            <p class="m-0 mt-1 text-md text-text leading-snug">{selectedLinkTask.summary}</p>
          </div>
          <span class="flex-1"></span>
          <div class="flex items-center justify-end gap-2">
            <button
              class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-border bg-transparent text-text-secondary text-md font-inherit cursor-pointer hover:bg-hover hover:text-text"
              onclick={() => (selectedLinkTask = null)}
            >
              <X size={13} />
              Cancel
            </button>
            <button
              class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg border-0 bg-accent-bg text-accent-text text-md font-inherit enabled:cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
              onclick={() => void confirmLink()}
              disabled={linking}
            >
              {#if linking}
                <LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" />
              {:else}
                <Link2 size={13} />
              {/if}
              Link
            </button>
          </div>
        {:else}
          <TaskListPicker
            trackerId={connectionId}
            repoRoot={cfgRoot}
            onPick={onPickTask}
            displayLimit={DISPLAY_LIMIT}
            autofocusSearch
            onActivity={clearTaskSendFeedback}
            bind:filteredCount
          >
            {#snippet rowBadge(task)}
              {#if mode === 'link' && (linkedKeys.has(task.key) || branchTaskKeys.has(task.key))}
                <span
                  class="flex-shrink-0 px-1.5 py-px rounded-md bg-success-bg text-2xs text-success-text"
                  title={branchTaskKeys.has(task.key)
                    ? 'Tracked via the branch name of this worktree'
                    : 'Already linked to this worktree'}>Linked</span
                >
              {/if}
            {/snippet}
            {#snippet rowActions(task)}
              {#if mode === 'link'}
                {#if linkedKeys.has(task.key) || branchTaskKeys.has(task.key)}
                  <button
                    class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-faint flex-shrink-0 opacity-60 transition-opacity duration-fast enabled:cursor-pointer enabled:hover:opacity-100 enabled:hover:bg-danger-bg enabled:hover:text-danger-text disabled:cursor-not-allowed disabled:opacity-30"
                    onclick={(e) => {
                      e.stopPropagation()
                      void unlinkTask($state.snapshot(task) as TrackerTaskLite)
                    }}
                    disabled={linking || branchTaskKeys.has(task.key)}
                    title={branchTaskKeys.has(task.key)
                      ? 'This task key is part of the branch name — the link comes from the branch and cannot be removed'
                      : 'Unlink this task from the worktree'}
                    aria-label="Unlink task"
                  >
                    <Unlink size={12} />
                  </button>
                {/if}
              {:else}
                {#if hasActiveAgent}
                  <button
                    class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-faint cursor-pointer flex-shrink-0 opacity-0 transition-opacity duration-fast group-hover/task:opacity-100 hover:bg-hover-strong hover:text-generate"
                    onclick={(e) => sendTaskToAgent($state.snapshot(task) as TrackerTaskLite, e)}
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
                    void copyTaskToClipboard($state.snapshot(task) as TrackerTaskLite, e)
                  }}
                  title="Copy to clipboard"
                  aria-label="Copy task to clipboard"
                >
                  <Copy size={12} />
                </button>
              {/if}
            {/snippet}
            {#snippet banner()}
              {#if sendStatus || sendError}
                <div
                  class="flex items-start gap-2 px-2.5 py-2 rounded-md text-xs leading-snug"
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
            {/snippet}
          </TaskListPicker>
        {/if}
      </div>

      {#if showsTaskList}
        <div class="flex items-center justify-between px-4 py-2 border-t border-border-subtle">
          <span class="text-xs text-text-faint"
            >↑↓ navigate · Enter {mode === 'link' ? 'pick' : 'select'} · Esc close</span
          >
          <span class="text-xs text-text-muted"
            >{filteredCount > DISPLAY_LIMIT
              ? `${DISPLAY_LIMIT} of ${filteredCount} tasks`
              : `${filteredCount} task${filteredCount !== 1 ? 's' : ''}`}</span
          >
        </div>
      {/if}
    {/if}
  </div>
</div>
