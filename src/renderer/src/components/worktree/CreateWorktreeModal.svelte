<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { ProgressAddon, type IProgressState } from '@xterm/addon-progress'
  import '@xterm/xterm/css/xterm.css'
  import { workspaceState, selectWorktree } from '../../lib/stores/workspace.svelte'
  import { getPref, prefs } from '../../lib/stores/preferences.svelte'
  import { openTool } from '../../lib/stores/tabs.svelte'
  import {
    getResolvedConfig,
    getTrackerCredentials,
    setActiveTask,
  } from '../../lib/stores/taskTracker.svelte'
  import { statusChipClass } from '../../lib/taskTracker/statusChip'
  import { unlockSizeOnResize } from '../../lib/actions/resizableDialog'
  import { Pencil, Plus, X } from '@lucide/svelte'
  import { taskDisplayKey } from '../../lib/taskTracker/taskFilterPrefs'
  import type { TrackerTaskLite } from '../../lib/taskTracker/types'
  import TaskListPicker from '../taskTracker/TaskListPicker.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import NewTaskForm from '../taskTracker/NewTaskForm.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import type { TrackerProviderKind } from '../../lib/taskTracker/types'
  import { getTheme } from '../../lib/terminal/themes'
  import { safeDirName } from '../../lib/sanitize'
  import { prStateChip } from '../../lib/github/prState'
  import BranchPicker from './BranchPicker.svelte'
  import { isRemoteOnly } from './utils'

  let {
    onClose,
    repoRoot: repoRootProp,
    workspaceId: workspaceIdProp,
    baseBranch: baseBranchProp,
  }: {
    onClose: () => void
    repoRoot?: string
    workspaceId?: string
    baseBranch?: string
  } = $props()

  type Step = 'loading' | 'pickBase' | 'creating' | 'setup' | 'done' | 'error'
  type Mode = 'new' | 'existing' | 'task' | 'newTask'

  let step = $state<Step>('loading')
  let mode = $state<Mode>('new')
  let branches = $state<{ local: string[]; remote: string[] }>({ local: [], remote: [] })
  let branchQuery = $state('')
  let selectedBase = $state('')
  let newBranchName = $state('')
  let errorMessage = $state('')
  let createdPath = $state('')
  let homedir = $state('')
  let refreshing = $state(false)
  let containerEl: HTMLDivElement | undefined = $state()

  // "From task" mode: the picked task + the branch name generated from it (the task list
  // itself — projects, filters, search — lives in the shared TaskListPicker).
  let selectedTask = $state<TrackerTaskLite | null>(null)
  let taskBranchName = $state('')
  let taskBranchEdited = $state(false)

  let setupLabel = $state('')
  let setupCurrent = $state(0)
  let setupTotal = $state(0)
  let setupErrors = $state<string[]>([])
  let cleanupProgressListener: (() => void) | null = null

  // The selected-task screen is much shorter than the list screen. If the dialog carries an
  // explicit height (user resize writes one inline), picking a task would leave a large void —
  // drop the inline height so the dialog hugs its content, and restore it when the pick is
  // cleared and the tall list comes back.
  let savedInlineHeight = ''
  $effect(() => {
    if (!containerEl) return
    if (selectedTask) {
      savedInlineHeight = containerEl.style.height
      containerEl.style.height = ''
    } else if (savedInlineHeight) {
      containerEl.style.height = savedInlineHeight
      savedInlineHeight = ''
    }
  })

  let setupTerm: Terminal | null = null
  let progressState = $state(0)
  let progressValue = $state(0)
  let finishTimer: ReturnType<typeof setTimeout> | null = null

  let repoRoot = $derived(repoRootProp ?? workspaceState.repoRoot!)
  // Tracker config (.canopy/config.json) is resolved against the ACTIVE WORKTREE — that's where
  // the Project tracker modal reads/writes it. Using the main repo root here would silently pick
  // up a stale copy of the config from the default branch.
  let trackerRepoRoot = $derived(workspaceState.selectedWorktreePath ?? repoRoot)
  let projectName = $derived(repoRoot.split('/').pop() || 'project')
  let workspaceId = $derived(workspaceIdProp ?? workspaceState.workspace?.id)

  let effectiveBranchName = $derived(
    mode === 'new'
      ? newBranchName
      : mode === 'task' || mode === 'newTask'
        ? taskBranchName
        : selectedBase && isRemoteOnly(selectedBase, branches)
          ? selectedBase.slice(selectedBase.indexOf('/') + 1)
          : selectedBase,
  )

  let worktreeDir = $derived.by(() => {
    if (!effectiveBranchName) return ''
    const baseDir = getPref('worktrees.baseDir', '~/canopy/worktrees')
    const safeName = safeDirName(effectiveBranchName)
    return `${baseDir}/${projectName}/${safeName}`
  })

  let worktreeDirDisplay = $derived.by(() => {
    const p = homedir && worktreeDir.startsWith('~/') ? homedir + worktreeDir.slice(1) : worktreeDir
    // Consistent separators for display — homedir arrives with backslashes on Windows while the
    // configured base dir uses forward slashes, which rendered as C:\Users\x/canopy/….
    return window.api.platform === 'win32' ? p.replace(/\//g, '\\') : p.replace(/\\/g, '/')
  })

  // Resolved default base (from settings) that actually exists in this repo, and the configured
  // name when it doesn't — the latter drives a hint above the picker.
  let defaultBase = $state('')
  let defaultBaseMissing = $state('')

  function resolveDefaultBase(name: string): string {
    if (branches.local.includes(name)) return name
    if (branches.remote.includes(name)) return name
    return branches.remote.find((r) => r.slice(r.indexOf('/') + 1) === name) ?? ''
  }

  // Restore focus to the element that opened the modal when it closes. Captured
  // in the script body rather than in onMount because that callback is async —
  // Svelte does not treat a returned promise as a cleanup function.
  const previouslyFocused = document.activeElement as HTMLElement | null
  onDestroy(() => previouslyFocused?.focus?.())

  onMount(async () => {
    containerEl?.focus()
    window.api.getHomedir().then((h) => (homedir = h))
    try {
      const list = await window.api.worktreeListBranches({ repoRoot })
      branches = { local: list.local, remote: list.remote }
      if (baseBranchProp) {
        selectedBase = baseBranchProp
      } else {
        const preferred = getPref('worktrees.defaultBaseBranch', '').trim()
        if (preferred) {
          defaultBase = resolveDefaultBase(preferred)
          if (defaultBase) selectedBase = defaultBase
          else defaultBaseMissing = preferred
        }
      }
      step = 'pickBase'
    } catch (e) {
      errorMessage = e instanceof Error ? e.message : String(e)
      step = 'error'
    }
  })

  async function refreshBranches(): Promise<void> {
    refreshing = true
    try {
      const list = await window.api.worktreeRefreshBranches({ repoRoot })
      branches = { local: list.local, remote: list.remote }
    } catch {
      // fetch failed — keep existing branch list
    }
    refreshing = false
  }

  onDestroy(() => {
    if (finishTimer) clearTimeout(finishTimer)
    window.api.abortWorktreeSetup()
    cleanupProgressListener?.()
    disposeSetupTerminal()
  })

  function initSetupTerminal(container: HTMLDivElement): void {
    if (setupTerm) return
    const currentTheme = getTheme(prefs.theme || 'Default')
    const term = new Terminal({
      fontSize: 11,
      fontFamily: 'JetBrains Mono, JetBrainsMono Nerd Font, Fira Code, Menlo, monospace',
      theme: currentTheme,
      scrollback: 1000,
      disableStdin: true,
      cursorBlink: false,
      cursorInactiveStyle: 'none',
    })
    const fitAddon = new FitAddon()
    const progressAddon = new ProgressAddon()
    term.open(container)
    term.loadAddon(fitAddon)
    term.loadAddon(progressAddon)
    progressAddon.onChange(({ state, value }: IProgressState) => {
      progressState = state
      progressValue = value
    })
    requestAnimationFrame(() => fitAddon.fit())
    setupTerm = term
  }

  function disposeSetupTerminal(): void {
    if (setupTerm) {
      setupTerm.dispose()
      setupTerm = null
      progressState = 0
      progressValue = 0
    }
  }

  let branchNameError = $derived.by(() => {
    if (!newBranchName) return null
    if (/\s/.test(newBranchName)) return 'No spaces allowed'
    if (/\.\./.test(newBranchName)) return 'Cannot contain ..'
    if (/[~^:\\]/.test(newBranchName)) return 'Invalid characters'
    if (newBranchName.startsWith('-')) return 'Cannot start with -'
    if (worktreeBranches.has(newBranchName)) {
      return 'Branch is already checked out in an existing worktree'
    }
    if (branches.local.includes(newBranchName)) return 'Branch already exists'
    return null
  })

  let taskBranchNameError = $derived.by(() => {
    if (!taskBranchName) return null
    if (/\s/.test(taskBranchName)) return 'No spaces allowed'
    if (/\.\./.test(taskBranchName)) return 'Cannot contain ..'
    if (/[~^:\\]/.test(taskBranchName)) return 'Invalid characters'
    if (taskBranchName.startsWith('-')) return 'Cannot start with -'
    if (worktreeBranches.has(taskBranchName)) {
      return 'Branch is already checked out in an existing worktree'
    }
    if (branches.local.includes(taskBranchName)) return 'Branch already exists'
    return null
  })

  // Branches already checked out by this project's worktrees — git refuses to check them out
  // twice, so creation is blocked up front.
  let worktreeBranches = $derived.by(() => {
    // Case-insensitive: Windows paths arrive with mixed drive-letter casing depending on source.
    const norm = (p: string | null | undefined): string =>
      (p ?? '').replace(/\\/g, '/').toLowerCase()
    if (norm(repoRoot) !== norm(workspaceState.repoRoot)) return new Set<string>()
    return new Set(workspaceState.worktrees.map((w) => w.branch).filter((b): b is string => !!b))
  })

  let existingModeError = $derived(
    mode === 'existing' && selectedBase && worktreeBranches.has(effectiveBranchName)
      ? 'This branch is already checked out in an existing worktree'
      : null,
  )

  function hasSetupConfig(): boolean {
    if (!workspaceId) return false
    const raw = getPref(`workspace:${workspaceId}:worktreeSetup`, '')
    if (!raw) return false
    try {
      const actions = JSON.parse(raw) as unknown[]
      return Array.isArray(actions) && actions.length > 0
    } catch {
      return false
    }
  }

  function setMode(next: Mode): void {
    if (mode === next) return
    mode = next
    // Base-taking modes start from the configured default base (when it exists in this repo).
    selectedBase = next === 'existing' ? '' : defaultBase
    newBranchName = ''
    selectedTask = null
    taskBranchName = ''
    taskBranchEdited = false
  }

  // "From task" availability. The task list binds to the ACTIVE project's tracker config, hence
  // the same-project requirement.
  let taskModeState = $derived.by(() => {
    const trackers = getResolvedConfig()?.config.trackers ?? []
    const creds = getTrackerCredentials()
    const usable = trackers.find(
      (t) => (creds[t.id]?.hasToken ?? false) && creds[t.id]?.valid !== false,
    )
    const norm = (p: string | null | undefined): string => (p ?? '').replace(/\\/g, '/')
    if (norm(repoRoot) !== norm(workspaceState.repoRoot)) {
      return { disabled: true, reason: 'Switch to this project first', trackerId: '' }
    }
    if (trackers.length === 0) {
      return { disabled: true, reason: 'No tracker configured for this project', trackerId: '' }
    }
    if (!usable) {
      return { disabled: true, reason: 'Tracker credentials missing or expired', trackerId: '' }
    }
    return { disabled: false, reason: '', trackerId: usable.id }
  })

  async function pickTask(task: TrackerTaskLite): Promise<void> {
    selectedTask = $state.snapshot(task) as TrackerTaskLite
    taskBranchEdited = false
    await updateTaskBranchPreview()
  }

  let trackerProvider = $derived(
    (getResolvedConfig()?.config.trackers.find((t) => t.id === taskModeState.trackerId)?.provider ??
      'jira') as TrackerProviderKind,
  )

  // One click creates BOTH: the tracker task and the worktree named by the (pre-create) branch
  // draft. The confirm pane is a fallback only — it appears when the resolved name turns out
  // invalid or already exists, so the user can fix it and hit Create.
  async function handleTaskCreated(
    task: TrackerTaskLite,
    warnings: string[],
    branchDraft?: string,
  ): Promise<void> {
    for (const w of warnings) addToast(w)
    selectedTask = $state.snapshot(task) as TrackerTaskLite
    taskBranchName = (branchDraft || task.key).replaceAll('{taskKey}', task.key)
    taskBranchEdited = true
    if (taskBranchNameError) {
      addToast(`Task ${task.key} created — adjust the branch name: ${taskBranchNameError}`)
      return
    }
    await createWorktreeFromTask()
  }

  async function updateTaskBranchPreview(): Promise<void> {
    if (!selectedTask || taskBranchEdited) return
    try {
      const result = await window.api.taskTrackerPrepareBranchFromTask({
        connectionId: taskModeState.trackerId,
        task: $state.snapshot(selectedTask) as TrackerTaskLite,
        repoRoot: trackerRepoRoot,
      })
      taskBranchName = result.branchName
    } catch {
      taskBranchName = selectedTask.key
    }
  }

  // Branches (local + remote) that already reference the selected task's key — creating another
  // branch for the same task silently forks the work, so surface them with a checkout option.
  let taskExistingBranches = $derived.by(() => {
    if (!selectedTask?.key) return []
    // Boundary match so GAKKO-74 does not hit GAKKO-743.
    const re = new RegExp(
      `(^|[^A-Za-z0-9])${selectedTask.key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![0-9])`,
      'i',
    )
    const found: Record<string, { name: string; ref: string; remoteOnly: boolean }> = {}
    for (const b of branches.local) {
      if (re.test(b)) found[b] = { name: b, ref: b, remoteOnly: false }
    }
    for (const r of branches.remote) {
      const name = r.slice(r.indexOf('/') + 1)
      if (re.test(name) && !(name in found)) found[name] = { name, ref: r, remoteOnly: true }
    }
    return Object.values(found).slice(0, 5)
  })

  // PR state chips for those branches, via the gh CLI (best effort).
  let taskBranchPRs = $state<Record<string, { number: number; state: string; isDraft: boolean }>>(
    {},
  )
  $effect(() => {
    const list = taskExistingBranches
    taskBranchPRs = {}
    if (list.length === 0) return
    let cancelled = false
    for (const b of list.slice(0, 3)) {
      void window.api
        .taskTrackerPRDetails(trackerRepoRoot, b.name)
        .then((pr) => {
          if (!cancelled && pr) {
            taskBranchPRs = {
              ...taskBranchPRs,
              [b.name]: { number: pr.number, state: pr.state, isDraft: pr.isDraft },
            }
          }
        })
        .catch(() => {})
    }
    return () => {
      cancelled = true
    }
  })

  // Check out an existing task branch instead of creating a duplicate — same path as the
  // "From existing branch" mode, plus the task link.
  async function checkoutExistingTaskBranch(b: {
    name: string
    ref: string
    remoteOnly: boolean
  }): Promise<void> {
    if (!selectedTask || worktreeBranches.has(b.name)) return
    step = 'creating'
    try {
      const baseDir = getPref('worktrees.baseDir', '~/canopy/worktrees')
      let path = `${baseDir}/${projectName}/${safeDirName(b.name)}`
      if (path.startsWith('~/')) path = (homedir + path.slice(1)).replace(/\\/g, '/')
      const created = await window.api.worktreeCreate({
        repoRoot,
        worktreePath: path,
        mode: 'existing',
        branch: b.remoteOnly ? b.ref : b.name,
        createLocalTracking: b.remoteOnly,
      })
      createdPath = created.worktreePath
      await setActiveTask(created.worktreePath, {
        taskKey: selectedTask.key,
        summary: selectedTask.summary,
        connectionId: taskModeState.trackerId,
      })
      if (hasSetupConfig() && workspaceId) {
        step = 'setup'
        await runSetup()
      } else {
        finishCreation()
      }
    } catch (err) {
      errorMessage = err instanceof Error ? err.message : String(err)
      step = 'error'
    }
  }

  async function createWorktreeFromTask(): Promise<void> {
    if (!selectedTask || !taskBranchName || taskBranchNameError || !selectedBase) return
    step = 'creating'
    try {
      // taskTracker:createWorktreeFromTask does not expand `~` — resolve it here.
      const worktreePath = worktreeDir.startsWith('~/')
        ? (homedir + worktreeDir.slice(1)).replace(/\\/g, '/')
        : worktreeDir
      const created = await window.api.taskTrackerCreateWorktreeFromTask({
        connectionId: taskModeState.trackerId,
        task: $state.snapshot(selectedTask) as TrackerTaskLite,
        repoRoot,
        worktreePath,
        baseBranch: selectedBase,
        branchName: taskBranchName,
      })
      createdPath = created.worktreePath
      await setActiveTask(created.worktreePath, {
        taskKey: selectedTask.key,
        summary: selectedTask.summary,
        connectionId: taskModeState.trackerId,
      })

      if (hasSetupConfig() && workspaceId) {
        step = 'setup'
        await runSetup()
      } else {
        finishCreation()
      }
    } catch (err) {
      errorMessage = err instanceof Error ? err.message : String(err)
      step = 'error'
    }
  }

  async function createWorktree(): Promise<void> {
    if (!newBranchName || branchNameError || !selectedBase) return
    step = 'creating'
    try {
      const created = await window.api.worktreeCreate({
        repoRoot,
        worktreePath: worktreeDir,
        mode: 'new',
        branch: newBranchName,
        baseBranch: selectedBase,
      })
      createdPath = created.worktreePath

      if (hasSetupConfig() && workspaceId) {
        step = 'setup'
        await runSetup()
      } else {
        finishCreation()
      }
    } catch (err) {
      errorMessage = err instanceof Error ? err.message : String(err)
      step = 'error'
    }
  }

  async function createWorktreeFromExisting(): Promise<void> {
    if (!selectedBase || existingModeError) return
    step = 'creating'
    try {
      const createLocalTracking = isRemoteOnly(selectedBase, branches)
      const created = await window.api.worktreeCreate({
        repoRoot,
        worktreePath: worktreeDir,
        mode: 'existing',
        branch: selectedBase,
        createLocalTracking,
      })
      createdPath = created.worktreePath

      if (hasSetupConfig() && workspaceId) {
        step = 'setup'
        await runSetup()
      } else {
        finishCreation()
      }
    } catch (err) {
      errorMessage = err instanceof Error ? err.message : String(err)
      step = 'error'
    }
  }

  async function runSetup(): Promise<void> {
    cleanupProgressListener = window.api.onWorktreeSetupProgress((data) => {
      setupLabel = data.label
      setupCurrent = data.actionIndex + 1
      setupTotal = data.totalActions
      if (data.outputChunk && setupTerm) {
        setupTerm.write(data.outputChunk)
      }
      if (data.status === 'error' && data.error) {
        setupErrors = [...setupErrors, `${data.label}: ${data.error}`]
      }
    })

    try {
      await window.api.runWorktreeSetup(workspaceId!, repoRoot, createdPath || worktreeDirDisplay)
    } catch (err) {
      setupErrors = [...setupErrors, err instanceof Error ? err.message : String(err)]
    }

    cleanupProgressListener?.()
    cleanupProgressListener = null
    finishCreation()
  }

  function finishCreation(): void {
    if (step === 'done') return
    step = 'done'
    if (finishTimer) clearTimeout(finishTimer)
    finishTimer = setTimeout(
      async () => {
        finishTimer = null
        const targetPath = createdPath || worktreeDirDisplay
        try {
          await selectWorktree(targetPath)
          await openTool(getPref('newWorktree.toolId', 'shell'), targetPath).catch((err) => {
            console.error('Failed to launch tool after worktree creation:', err)
          })
          onClose()
        } catch (err) {
          errorMessage = err instanceof Error ? err.message : String(err)
          step = 'error'
        }
      },
      setupErrors.length > 0 ? 2000 : 400,
    )
  }

  function skipSetup(): void {
    window.api.abortWorktreeSetup()
    cleanupProgressListener?.()
    cleanupProgressListener = null
    disposeSetupTerminal()
    finishCreation()
  }

  function setupTerminalAction(node: HTMLDivElement): { destroy: () => void } {
    initSetupTerminal(node)
    return { destroy: disposeSetupTerminal }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Tab' && containerEl) {
      const focusable = containerEl.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement as HTMLElement | null
      if (
        e.shiftKey &&
        (active === first || active === containerEl || !containerEl.contains(active))
      ) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      if (step === 'setup') {
        skipSetup()
      } else {
        onClose()
      }
    }
  }

  const inputCls =
    'w-full border border-border rounded-lg bg-bg-input text-text text-md font-inherit px-2.5 py-2 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint'
  const btnCancelCls =
    'px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text transition-colors duration-fast hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1 disabled:opacity-40 disabled:cursor-default'
  const btnPrimaryCls =
    'px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text transition-colors duration-fast enabled:hover:bg-accent-muted focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1 disabled:opacity-40 disabled:cursor-default'
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-[1001] flex justify-center items-start pt-20 bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={() => (step === 'setup' ? skipSetup() : onClose())}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none resize w-[576px] min-w-[480px] max-w-[94vw] min-h-[200px] max-h-[680px] flex flex-col bg-bg-overlay border border-border rounded-[10px] shadow-[0_16px_48px_var(--color-scrim)] overflow-hidden"
    use:unlockSizeOnResize
    role="dialog"
    aria-modal="true"
    aria-labelledby="create-worktree-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3
      id="create-worktree-title"
      class="m-0 px-5 pt-4 pb-3 text-[15px] font-semibold text-text flex-shrink-0"
    >
      Create worktree based on
    </h3>

    <!-- Selected-task pane shared by the From-task and New-task modes: card, duplicate-branch
       warning, template-generated (editable) branch name and the Create action. -->
    {#snippet selectedTaskPane()}
      {#if selectedTask}
        <p
          class="m-0 text-md text-text-secondary cursor-help"
          title="This task will be linked to the new worktree — the Task panel tracks it by default (status, comments)"
        >
          Selected task
        </p>
        <div
          class="px-3 py-2.5 bg-bg-input border border-border rounded-xl"
          title="This task will be linked to the new worktree — the Task panel tracks it by default (status, comments)"
        >
          <div class="flex items-center gap-2">
            <span class="font-semibold text-sm text-accent-text"
              >{taskDisplayKey(selectedTask)}</span
            >
            {#if selectedTask.status}
              <span
                class="text-2xs px-1.5 py-px rounded-md {statusChipClass(
                  selectedTask.statusCategory,
                )}">{selectedTask.status}</span
              >
            {/if}
            <span class="flex-1"></span>
            <button
              class="flex items-center justify-center size-5 border-0 rounded-md bg-transparent text-text-muted text-sm leading-none cursor-pointer p-0 hover:bg-hover hover:text-text"
              onclick={() => {
                selectedTask = null
                taskBranchName = ''
                taskBranchEdited = false
              }}
              title="Choose a different task"
              aria-label="Change task">×</button
            >
          </div>
          <p class="m-0 mt-1 text-md text-text leading-snug">{selectedTask.summary}</p>
        </div>
        {#if taskExistingBranches.length > 0}
          <!-- The task already has work on a branch — creating a fresh one from the template
                       would silently fork it (and orphan any open PR). -->
          <div
            class="flex flex-col gap-1.5 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
          >
            <span class="text-xs text-text-secondary leading-snug">
              This task already has {taskExistingBranches.length === 1 ? 'a branch' : 'branches'} — check
              it out instead of creating a duplicate that will diverge:
            </span>
            {#each taskExistingBranches as b (b.ref)}
              {@const pr = taskBranchPRs[b.name]}
              {@const checkedOut = worktreeBranches.has(b.name)}
              <div class="flex items-center gap-2 min-w-0">
                <span class="font-mono text-xs text-text truncate" title={b.ref}>{b.name}</span>
                {#if b.remoteOnly}
                  <span class="px-1.5 py-px rounded-md text-2xs bg-active text-text-muted shrink-0"
                    >remote</span
                  >
                {/if}
                {#if pr}
                  {@const chip = prStateChip(pr.state, pr.isDraft)}
                  <span class="px-1.5 py-px rounded-md text-2xs shrink-0 {chip.cls}"
                    >PR #{pr.number} · {chip.label}</span
                  >
                {/if}
                <span class="flex-1"></span>
                <button
                  class="shrink-0 px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit enabled:cursor-pointer enabled:hover:border-accent-muted enabled:hover:text-accent-text disabled:opacity-50 disabled:cursor-default"
                  onclick={() => checkoutExistingTaskBranch(b)}
                  disabled={checkedOut}
                  title={checkedOut
                    ? 'Already checked out in an existing worktree'
                    : `Create the worktree on ${b.name} instead of a new branch`}
                >
                  Check out
                </button>
              </div>
            {/each}
          </div>
        {/if}
        <label
          for="create-wt-task-branch"
          class="block text-xs font-semibold tracking-[0.5px] text-text-muted uppercase"
        >
          Branch name
        </label>
        <input
          id="create-wt-task-branch"
          class="{inputCls} font-mono"
          type="text"
          bind:value={taskBranchName}
          oninput={() => (taskBranchEdited = true)}
          spellcheck="false"
          autocomplete="off"
          onkeydown={(e) => {
            if (e.key === 'Enter' && taskBranchName && !taskBranchNameError) {
              e.preventDefault()
              createWorktreeFromTask()
            }
          }}
        />
        {#if taskBranchNameError}
          <p class="m-0 text-sm text-danger-text">{taskBranchNameError}</p>
        {/if}
        <p class="m-0 text-xs text-text-muted leading-snug">
          Generated from the branch naming template in this project's tracker configuration (Project
          management → ⚙ in the sidebar). Edit freely — the template is just the default.
        </p>
        {#if worktreeDir}
          <p class="m-0 text-xs text-text-faint font-mono break-all">
            Path: {worktreeDirDisplay}
          </p>
        {/if}
        <div class="flex justify-end gap-2 mt-2">
          <button class="{btnCancelCls} inline-flex items-center gap-1.5" onclick={onClose}>
            <X size={14} />
            Cancel
          </button>
          <button
            class="{btnPrimaryCls} inline-flex items-center gap-1.5"
            onclick={createWorktreeFromTask}
            disabled={!taskBranchName || !!taskBranchNameError}
          >
            <Plus size={14} />
            Create
          </button>
        </div>
      {/if}
    {/snippet}
    {#if step === 'loading'}
      <div
        class="px-5 pb-5 flex-1 overflow-y-auto min-h-0 flex flex-col items-center justify-center py-8 gap-2"
      >
        <p class="text-md text-text-secondary m-0">Loading branches...</p>
      </div>
    {:else if step === 'pickBase'}
      <div class="px-5 pb-5 flex-1 min-h-0 flex flex-col">
        <!-- The mode switch stays visible on every pick-base screen — also when the default base
             branch skipped the picker — so the creation type can always be changed. -->
        <div
          class="flex gap-0.5 p-0.5 mb-3 bg-active rounded-lg shrink-0"
          role="group"
          aria-label="Branch mode"
        >
          <button
            class="flex-1 px-2 py-[5px] border-0 rounded-md text-sm font-inherit cursor-pointer transition-all duration-fast {mode ===
            'new'
              ? '!bg-bg-overlay !text-text shadow-[0_1px_2px_var(--color-scrim)]'
              : 'bg-transparent text-text-muted hover:text-text-secondary'}"
            onclick={() => setMode('new')}
            aria-pressed={mode === 'new'}
            type="button"
          >
            New branch
          </button>
          <div class="w-px my-1 bg-border shrink-0" aria-hidden="true"></div>
          <button
            class="flex-1 px-2 py-[5px] border-0 rounded-md text-sm font-inherit cursor-pointer transition-all duration-fast {mode ===
            'existing'
              ? '!bg-bg-overlay !text-text shadow-[0_1px_2px_var(--color-scrim)]'
              : 'bg-transparent text-text-muted hover:text-text-secondary'}"
            onclick={() => setMode('existing')}
            aria-pressed={mode === 'existing'}
            type="button"
          >
            Existing branch
          </button>
          <div class="w-px my-1 bg-border shrink-0" aria-hidden="true"></div>
          <button
            class="flex-1 px-2 py-[5px] border-0 rounded-md text-sm font-inherit transition-all duration-fast enabled:cursor-pointer disabled:opacity-40 disabled:cursor-default {mode ===
            'task'
              ? '!bg-bg-overlay !text-text shadow-[0_1px_2px_var(--color-scrim)]'
              : 'bg-transparent text-text-muted enabled:hover:text-text-secondary'}"
            onclick={() => setMode('task')}
            disabled={taskModeState.disabled}
            aria-pressed={mode === 'task'}
            type="button"
            title={taskModeState.disabled
              ? taskModeState.reason
              : 'Pick a tracker task — the branch name is generated from it'}
          >
            <span class="inline-flex items-center justify-center gap-1.5">
              {#if !taskModeState.disabled}<TrackerProviderIcon provider={trackerProvider} />{/if}
              Existing task
            </span>
          </button>
          <div class="w-px my-1 bg-border shrink-0" aria-hidden="true"></div>
          <button
            class="flex-1 px-2 py-[5px] border-0 rounded-md text-sm font-inherit transition-all duration-fast enabled:cursor-pointer disabled:opacity-40 disabled:cursor-default {mode ===
            'newTask'
              ? '!bg-bg-overlay !text-text shadow-[0_1px_2px_var(--color-scrim)]'
              : 'bg-transparent text-text-muted enabled:hover:text-text-secondary'}"
            onclick={() => setMode('newTask')}
            disabled={taskModeState.disabled}
            aria-pressed={mode === 'newTask'}
            type="button"
            title={taskModeState.disabled
              ? taskModeState.reason
              : 'Create a task in the tracker, then a worktree for it'}
          >
            <span class="inline-flex items-center justify-center gap-1.5">
              {#if !taskModeState.disabled}<TrackerProviderIcon provider={trackerProvider} />{/if}
              New task
            </span>
          </button>
        </div>
        {#if mode === 'new' && selectedBase}
          <div class="flex-1 overflow-y-auto min-h-0">
            <p
              class="m-0 mb-3 pb-2 border-b border-border-subtle text-md text-text-secondary flex items-center gap-1.5"
            >
              <span>Base branch: <strong class="text-text">{selectedBase}</strong></span>
              <button
                type="button"
                class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
                onclick={() => (selectedBase = '')}
                aria-label="Change base branch"
                title="Change the base branch — the default is set in Settings → Git → Worktrees"
              >
                <Pencil size={12} />
              </button>
            </p>
            <label
              for="create-wt-branch-name"
              class="block text-xs font-semibold tracking-[0.5px] text-text-muted uppercase"
            >
              New branch name
            </label>
            <input
              id="create-wt-branch-name"
              class={inputCls}
              type="text"
              bind:value={newBranchName}
              placeholder="feature/my-branch"
              spellcheck="false"
              autocomplete="off"
              onkeydown={(e) => {
                if (e.key === 'Enter' && newBranchName && !branchNameError) {
                  e.preventDefault()
                  createWorktree()
                }
              }}
            />
            {#if branchNameError}
              <p class="mt-1.5 mb-0 text-sm text-danger-text">{branchNameError}</p>
            {/if}
            {#if worktreeDir}
              <p class="mt-1.5 mb-0 text-xs text-text-faint font-mono break-all">
                Path: {worktreeDirDisplay}
              </p>
            {/if}
            <div class="flex justify-end gap-2 mt-4">
              <button class="{btnCancelCls} inline-flex items-center gap-1.5" onclick={onClose}>
                <X size={14} />
                Cancel
              </button>
              <button
                class="{btnPrimaryCls} inline-flex items-center gap-1.5"
                onclick={createWorktree}
                disabled={!newBranchName || !!branchNameError}
              >
                <Plus size={14} />
                Create
              </button>
            </div>
          </div>
        {:else if mode === 'task' && selectedBase}
          <div class="flex-1 min-h-0 flex flex-col gap-2">
            <p
              class="m-0 mb-1 pb-2 border-b border-border-subtle text-md text-text-secondary flex items-center gap-1.5"
            >
              <span>Base branch: <strong class="text-text">{selectedBase}</strong></span>
              <button
                type="button"
                class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
                onclick={() => (selectedBase = '')}
                aria-label="Change base branch"
                title="Change the base branch — the default is set in Settings → Git → Worktrees"
              >
                <Pencil size={12} />
              </button>
            </p>
            {#if !selectedTask}
              <p class="m-0 text-md text-text-secondary">Select task</p>
              <TaskListPicker
                trackerId={taskModeState.trackerId}
                repoRoot={trackerRepoRoot}
                onPick={(t) => void pickTask(t)}
                showMeta={false}
                displayLimit={50}
              />
            {:else}
              {@render selectedTaskPane()}
            {/if}
          </div>
        {:else if mode === 'newTask' && selectedBase}
          <div class="flex-1 min-h-0 flex flex-col gap-2">
            <p
              class="m-0 mb-1 pb-2 border-b border-border-subtle text-md text-text-secondary flex items-center gap-1.5"
            >
              <span>Base branch: <strong class="text-text">{selectedBase}</strong></span>
              <button
                type="button"
                class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
                onclick={() => (selectedBase = '')}
                aria-label="Change base branch"
                title="Change the base branch — the default is set in Settings → Git → Worktrees"
              >
                <Pencil size={12} />
              </button>
            </p>
            {#if !selectedTask}
              <p class="m-0 text-md text-text-secondary">New task</p>
              <NewTaskForm
                trackerId={taskModeState.trackerId}
                repoRoot={trackerRepoRoot}
                provider={trackerProvider}
                onCreated={handleTaskCreated}
                onCancel={onClose}
                submitLabel="Create task and worktree"
                showBranchName
              />
            {:else}
              {@render selectedTaskPane()}
            {/if}
          </div>
        {:else}
          {#if defaultBaseMissing && mode !== 'existing'}
            <p
              class="m-0 mb-2 px-2.5 py-2 rounded-md border border-experimental-border bg-experimental-bg text-xs text-text-secondary leading-snug"
            >
              The default base branch <strong class="font-mono">{defaultBaseMissing}</strong> doesn't
              exist in this repository — pick a base branch below. The default is set in Settings → Git
              → Worktrees.
            </p>
          {/if}
          <BranchPicker
            {branches}
            bind:query={branchQuery}
            bind:selectedBranch={selectedBase}
            {refreshing}
            onRefresh={refreshBranches}
            label={mode === 'existing' ? 'Branch to check out' : 'Base branch'}
            showRemoteOnlyTag={mode === 'existing'}
            highlightPicked={mode === 'existing'}
            fillQueryOnPick={mode === 'existing'}
            onCommit={mode === 'existing' ? createWorktreeFromExisting : undefined}
          />
          {#if mode === 'existing'}
            {#if existingModeError}
              <p class="mt-1.5 mb-0 text-sm text-danger-text">{existingModeError}</p>
            {/if}
            {#if selectedBase && worktreeDir}
              <p class="mt-1.5 mb-0 text-xs text-text-faint font-mono break-all">
                Path: {worktreeDirDisplay}
              </p>
            {/if}
            <div class="flex justify-end gap-2 mt-4">
              <button class="{btnCancelCls} inline-flex items-center gap-1.5" onclick={onClose}>
                <X size={14} />
                Cancel
              </button>
              <button
                class="{btnPrimaryCls} inline-flex items-center gap-1.5"
                onclick={createWorktreeFromExisting}
                disabled={!selectedBase || !!existingModeError}
              >
                <Plus size={14} />
                Create
              </button>
            </div>
          {/if}
        {/if}
      </div>
    {:else if step === 'creating'}
      <div
        class="px-5 pb-5 flex-1 overflow-y-auto min-h-0 flex flex-col items-center justify-center py-8 gap-2"
      >
        <p class="text-md text-text-secondary m-0" role="status" aria-live="polite">
          Creating worktree...
        </p>
      </div>
    {:else if step === 'setup'}
      <div class="flex flex-col px-5 pb-5 gap-2 flex-1 overflow-y-auto min-h-0">
        <p class="text-md text-text-secondary m-0" role="status" aria-live="polite">
          Running setup... ({setupCurrent}/{setupTotal})
        </p>
        <p class="text-sm font-mono text-text-muted m-0">{setupLabel}</p>
        <div class="relative rounded-lg overflow-hidden border border-border-subtle">
          {#if progressState > 0}
            <div
              class="absolute top-0 left-0 h-0.5 z-[5] transition-[width] duration-slow ease-out"
              class:bg-accent={progressState !== 2 && progressState !== 4 && progressState !== 3}
              class:bg-danger={progressState === 2}
              class:bg-warning={progressState === 4}
              class:progress-indeterminate={progressState === 3}
              style:width={progressState === 3 ? '100%' : `${progressValue}%`}
            ></div>
          {/if}
          <div class="h-[220px] p-2 box-border" use:setupTerminalAction></div>
        </div>
        {#if setupErrors.length > 0}
          {#each setupErrors as err (err)}
            <p class="mt-1.5 mb-0 text-sm text-danger-text">{err}</p>
          {/each}
        {/if}
        <div class="flex justify-end gap-2 mt-4">
          <button class={btnCancelCls} onclick={skipSetup}>Skip</button>
        </div>
      </div>
    {:else if step === 'done'}
      <div
        class="px-5 pb-5 flex-1 overflow-y-auto min-h-0 flex flex-col items-center justify-center py-8 gap-2"
      >
        <p class="text-md text-success m-0" role="status" aria-live="polite">Worktree created</p>
        <p class="text-xs text-text-faint font-mono break-all m-0">{createdPath}</p>
        {#if setupErrors.length > 0}
          <div class="mt-2 flex flex-col gap-1 items-center">
            <p class="text-md text-warning-text m-0">Setup completed with warnings:</p>
            {#each setupErrors as err (err)}
              <p class="text-sm text-danger-text m-0">{err}</p>
            {/each}
          </div>
        {/if}
      </div>
    {:else if step === 'error'}
      <div
        class="px-5 pb-5 flex-1 overflow-y-auto min-h-0 flex flex-col items-center justify-center py-8 gap-2"
      >
        <div role="alert" class="contents">
          <p class="text-md text-danger-text m-0">Error</p>
          <p class="text-xs text-text-faint font-mono break-all m-0">{errorMessage}</p>
        </div>
        <div class="flex justify-end gap-2 mt-4">
          <button class={btnCancelCls} onclick={onClose}>Close</button>
        </div>
      </div>
    {/if}
  </div>
</div>
