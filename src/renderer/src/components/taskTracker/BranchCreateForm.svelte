<script lang="ts">
  import { onMount } from 'svelte'
  import { X, ExternalLink, ArrowLeft, RotateCcw } from '@lucide/svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
  import { getPref, setPref } from '../../lib/stores/preferences.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { workspaceState, selectWorktree } from '../../lib/stores/workspace.svelte'
  import { getTools, getToolAvailability } from '../../lib/stores/tools.svelte'
  import {
    focusSessionByPtyId,
    getAiSessions,
    isAiToolId,
    openTool,
  } from '../../lib/stores/tabs.svelte'
  import { agentSessions } from '../../lib/agents/agentState.svelte'
  import { setActiveTask } from '../../lib/stores/taskTracker.svelte'
  import {
    agentNotReadyOutcome,
    agentStartFailedOutcome,
    logTaskToAgentFailure,
    sendTaskToAgentContext,
    taskToAgentUserMessage,
    type TaskToAgentOutcome,
  } from '../../lib/taskTracker/taskToAgent'
  import { safeDirName } from '../../lib/sanitize'

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

  let {
    connectionId,
    selectedBoardId,
    task,
    onBack,
  }: {
    connectionId: string
    selectedBoardId: string
    task: Task
    onBack: () => void
  } = $props()

  let branchTypeOptions: string[] = $state([])
  let selectedBranchType = $state('feat')
  let resolvedBranchName = $state('')
  // Set once the user hand-edits the branch name; template re-renders then stop overwriting it.
  let branchEdited = $state(false)
  let creatingWorktree = $state(false)
  let templateHasBranchType = $state(false)
  let initialized = $state(false)
  let fullTask = $state<Task>(task)
  let selectedAgentId = $state(getPref('taskTracker.lastAgent', ''))
  let branches = $state<{ local: string[]; remote: string[] }>({ local: [], remote: [] })
  let selectedBaseBranch = $state('')
  let operationStatus = $state('')
  let operationError = $state('')
  let createdWorktreePath = $state('')
  let contextSendFailed = $state(false)

  let baseBranchGroups = $derived(
    [
      { label: 'Local', options: branches.local.map((b) => ({ value: b, label: b })) },
      { label: 'Remote', options: branches.remote.map((b) => ({ value: b, label: b })) },
    ].filter((g) => g.options.length > 0),
  )

  let availableAgents = $derived.by(() => {
    const tools = getTools()
    const avail = getToolAvailability()
    return tools.filter((t) => isAiToolId(t.id) && avail[t.id])
  })

  // Tracker config (.canopy/config.json) lives in the ACTIVE WORKTREE — same path the Project
  // tracker modal edits; the main repo root may hold a stale copy from the default branch.
  let cfgRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)

  async function init(): Promise<void> {
    const repoRoot = workspaceState.repoRoot
    const [typeInfo, foundTask, branchList] = await Promise.all([
      window.api
        .taskTrackerResolveBranchType(
          task.type,
          connectionId,
          selectedBoardId || undefined,
          cfgRoot || undefined,
        )
        .catch(() => null),
      window.api.taskTrackerFindTaskByKey(task.key).catch(() => null),
      repoRoot
        ? window.api.worktreeListBranches({ repoRoot }).catch(() => null)
        : Promise.resolve(null),
    ])

    if (foundTask) fullTask = foundTask as Task

    if (branchList) {
      branches = { local: branchList.local, remote: branchList.remote }
    }
    // Default the base to the currently active branch (the prior hard-coded
    // behaviour), but fall back to the repo's reported current branch, then
    // to the first available local branch so the picker is never empty.
    selectedBaseBranch =
      workspaceState.branch ?? branchList?.current ?? branches.local[0] ?? branches.remote[0] ?? ''

    if (typeInfo) {
      branchTypeOptions = typeInfo.options
      selectedBranchType = typeInfo.defaultType
      templateHasBranchType = typeInfo.hasBranchType
    } else {
      branchTypeOptions = ['feat', 'fix', 'refactor', 'chore', 'docs', 'test']
      selectedBranchType = 'feat'
      templateHasBranchType = false
    }

    await updateBranchPreview()
    initialized = true
  }

  async function updateBranchPreview(): Promise<void> {
    if (branchEdited) return
    try {
      const plain = $state.snapshot(task) as Task
      const result = await window.api.taskTrackerPrepareBranchFromTask({
        connectionId,
        task: plain,
        boardId: selectedBoardId || undefined,
        branchType: templateHasBranchType ? selectedBranchType : undefined,
        repoRoot: cfgRoot || '',
      })
      resolvedBranchName = result.branchName
    } catch {
      resolvedBranchName = task.key
    }
  }

  function regenerateBranchName(): void {
    branchEdited = false
    updateBranchPreview()
  }

  async function onBranchTypeChange(): Promise<void> {
    await updateBranchPreview()
  }

  function hasSetupConfig(): boolean {
    const wsId = workspaceState.workspace?.id
    if (!wsId) return false
    const raw = getPref(`workspace:${wsId}:worktreeSetup`, '')
    if (!raw) return false
    try {
      const actions = JSON.parse(raw) as unknown[]
      return Array.isArray(actions) && actions.length > 0
    } catch {
      return false
    }
  }

  async function confirmBranchCreation(): Promise<void> {
    if (createdWorktreePath && contextSendFailed) {
      if (!selectedAgentId) {
        operationStatus = 'Worktree created'
        operationError = 'Select an agent to retry sending the task.'
        return
      }
      creatingWorktree = true
      operationError = ''
      contextSendFailed = false
      const taskSnapshot = $state.snapshot(fullTask) as typeof fullTask
      const sent = await sendTaskContextToSelectedAgent(
        createdWorktreePath,
        selectedAgentId,
        connectionId,
        taskSnapshot,
        { reuseExistingAgent: true },
      )
      creatingWorktree = false
      if (sent) closeDialog()
      return
    }

    const repoRoot = workspaceState.repoRoot
    const baseBranch = selectedBaseBranch
    if (!repoRoot || !baseBranch || !resolvedBranchName) return

    const baseDir = getPref('worktrees.baseDir', '~/canopy/worktrees')
    const projectName = repoRoot.split(/[/\\]/).pop() || 'project'
    const safeBranchName = safeDirName(resolvedBranchName)
    const worktreeDir = `${baseDir}/${projectName}/${safeBranchName}`
    const homedir = await window.api.getHomedir()
    const worktreePath = worktreeDir.startsWith('~/')
      ? (homedir + worktreeDir.slice(1)).replace(/\\/g, '/')
      : worktreeDir

    creatingWorktree = true
    operationStatus = 'Creating worktree...'
    operationError = ''
    contextSendFailed = false
    createdWorktreePath = ''
    setPref('taskTracker.lastAgent', selectedAgentId)
    try {
      const branchTask = $state.snapshot(task) as Task
      const created = await window.api.taskTrackerCreateWorktreeFromTask({
        connectionId,
        task: branchTask,
        boardId: selectedBoardId || undefined,
        branchType: templateHasBranchType ? selectedBranchType : undefined,
        repoRoot,
        worktreePath,
        baseBranch,
        branchName: resolvedBranchName,
      })
      createdWorktreePath = created.worktreePath
      operationStatus = 'Worktree created'
      await setActiveTask(created.worktreePath, {
        taskKey: fullTask.key,
        summary: fullTask.summary,
        connectionId,
        boardId: selectedBoardId || undefined,
      })

      if (hasSetupConfig()) {
        const wsId = workspaceState.workspace!.id
        operationStatus = 'Running worktree setup...'
        addToast('Running worktree setup...')
        try {
          await window.api.runWorktreeSetup(wsId, repoRoot, created.worktreePath)
          addToast('Worktree setup complete')
        } catch (e) {
          addToast('Worktree setup failed: ' + (e instanceof Error ? e.message : String(e)))
        }
      }

      if (selectedAgentId) {
        const taskSnapshot = $state.snapshot(fullTask) as typeof fullTask
        const sent = await sendTaskContextToSelectedAgent(
          created.worktreePath,
          selectedAgentId,
          connectionId,
          taskSnapshot,
        )
        if (!sent) {
          creatingWorktree = false
          return
        }
      } else {
        operationStatus = 'Opening worktree...'
        await selectWorktree(created.worktreePath)
      }

      closeDialog()
    } catch (e) {
      creatingWorktree = false
      operationStatus = ''
      operationError = ''
      closeDialog()
      await new Promise((r) => setTimeout(r, 0))
      await confirm({
        title: 'Worktree Creation Failed',
        message: e instanceof Error ? e.message : 'Failed to create worktree',
        confirmLabel: 'OK',
      })
    }
  }

  async function sendTaskContextToSelectedAgent(
    worktreePath: string,
    agentId: string,
    connId: string,
    taskSnapshot: Task,
    options: { reuseExistingAgent?: boolean } = {},
  ): Promise<boolean> {
    try {
      await selectWorktree(worktreePath)
    } catch (error) {
      handleTaskToAgentFailure(agentStartFailedOutcome(error), {
        taskKey: taskSnapshot.key,
        connectionId: connId,
        selectedAgentId: agentId,
      })
      return false
    }

    const existingAgentSession = options.reuseExistingAgent
      ? getAiSessions(worktreePath).find((session) => session.toolId === agentId)
      : null

    let sessionId = existingAgentSession?.sessionId
    if (sessionId) {
      operationStatus = 'Focusing agent...'
      focusSessionByPtyId(sessionId)
    } else {
      operationStatus = 'Starting agent...'
      let tab: Awaited<ReturnType<typeof openTool>>
      try {
        tab = await openTool(agentId, worktreePath)
      } catch (error) {
        handleTaskToAgentFailure(agentStartFailedOutcome(error), {
          taskKey: taskSnapshot.key,
          connectionId: connId,
          selectedAgentId: agentId,
        })
        return false
      }
      const pane = tab.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
      sessionId = pane?.sessionId
    }

    if (!sessionId) {
      handleTaskToAgentFailure(agentNotReadyOutcome(), {
        taskKey: taskSnapshot.key,
        connectionId: connId,
        selectedAgentId: agentId,
      })
      return false
    }

    operationStatus = 'Waiting for agent...'
    const ready = await waitForAgentIdle(sessionId)
    if (!ready) {
      handleTaskToAgentFailure(agentNotReadyOutcome(sessionId), {
        taskKey: taskSnapshot.key,
        connectionId: connId,
        selectedAgentId: agentId,
        sessionId,
      })
      return false
    }

    operationStatus = 'Sending task to agent...'
    const outcome = await sendTaskToAgentContext({
      connectionId: connId,
      task: taskSnapshot,
      repoRoot: workspaceState.repoRoot ?? undefined,
      target: {
        worktreePath,
        sessionId,
      },
    })
    if (outcome.status !== 'sent') {
      handleTaskToAgentFailure(outcome, {
        taskKey: taskSnapshot.key,
        connectionId: connId,
        selectedAgentId: agentId,
        sessionId,
      })
      return false
    }

    operationStatus = 'Task sent to agent'
    addToast('Task sent to agent')
    return true
  }

  function handleTaskToAgentFailure(
    outcome: Exclude<TaskToAgentOutcome, { status: 'sent' }>,
    metadata: {
      taskKey: string
      connectionId: string
      selectedAgentId?: string
      sessionId?: string
    },
  ): void {
    contextSendFailed = true
    operationStatus = 'Worktree created'
    operationError = taskToAgentUserMessage(outcome)
    logTaskToAgentFailure(outcome, metadata)
  }

  async function waitForAgentIdle(sessionId: string, timeoutMs = 30000): Promise<boolean> {
    await new Promise((r) => setTimeout(r, 500))
    const start = Date.now()
    while (Date.now() - start < timeoutMs) {
      const session = agentSessions[sessionId]
      if (session?.status.type === 'idle') return true
      if (session?.status.type === 'ended' || session?.status.type === 'error') return false
      await new Promise((r) => setTimeout(r, 200))
    }
    return false
  }

  onMount(() => {
    init()
  })

  const iconBtnCls =
    'flex items-center justify-center w-7 h-7 border-0 rounded-md bg-transparent text-text-muted cursor-pointer flex-shrink-0 hover:bg-hover hover:text-text'
</script>

{#if initialized}
  <div class="flex items-center gap-2 px-4 pt-3.5 pb-2.5 border-b border-border-subtle">
    <button class={iconBtnCls} onclick={onBack} aria-label="Back">
      <ArrowLeft size={16} />
    </button>
    <h3 class="m-0 text-lg font-semibold text-text flex-1">Create Branch</h3>
    <button class={iconBtnCls} onclick={() => closeDialog()} aria-label="Close">
      <X size={16} />
    </button>
  </div>
  <div class="p-4 flex flex-col gap-3.5">
    <div class="px-3 py-2.5 bg-bg-input border border-border rounded-xl">
      <div class="flex items-center gap-2 mb-1">
        {#if fullTask.url}
          <button
            class="inline-flex items-center gap-1 font-semibold text-sm text-accent-text bg-transparent border-0 p-0 cursor-pointer font-inherit hover:underline"
            onclick={() => window.api.openExternal(fullTask.url!)}
          >
            {fullTask.key}
            <ExternalLink size={11} />
          </button>
        {:else}
          <span class="font-semibold text-sm text-accent-text">{fullTask.key}</span>
        {/if}
        {#if fullTask.status}
          <span class="text-2xs px-1.5 py-px rounded-md bg-active text-text-muted"
            >{fullTask.status}</span
          >
        {/if}
      </div>
      <p class="m-0 text-md text-text leading-snug">{fullTask.summary}</p>
      {#if fullTask.description}
        <p class="mt-1.5 mb-0 text-xs text-text-muted leading-snug max-h-[60px] overflow-y-auto">
          {fullTask.description}
        </p>
      {/if}
    </div>

    {#if templateHasBranchType}
      <div class="flex items-center gap-2.5">
        <span class="text-sm text-text-muted w-[50px] flex-shrink-0">Type</span>
        <CustomSelect
          value={selectedBranchType}
          options={branchTypeOptions.map((o) => ({ value: o, label: o }))}
          onchange={(v) => {
            selectedBranchType = v
            onBranchTypeChange()
          }}
          maxWidth="none"
        />
      </div>
    {/if}
    {#if baseBranchGroups.length > 0}
      <div class="flex items-center gap-2.5">
        <span class="text-sm text-text-muted w-[50px] flex-shrink-0">Base</span>
        <CustomSelect
          value={selectedBaseBranch}
          groups={baseBranchGroups}
          onchange={(v) => {
            selectedBaseBranch = v
          }}
          maxWidth="none"
        />
      </div>
    {/if}
    <div class="flex items-center gap-2.5">
      <span class="text-sm text-text-muted w-[50px] flex-shrink-0">Branch</span>
      <input
        class="text-sm text-accent-text bg-bg-input px-2.5 py-[5px] rounded-lg flex-1 min-w-0 font-mono border border-border outline-none focus:border-focus-ring"
        name="branchName"
        aria-label="Branch name"
        bind:value={resolvedBranchName}
        oninput={() => (branchEdited = true)}
        spellcheck="false"
        autocomplete="off"
        title="Edit the branch name before creating it"
      />
      {#if branchEdited}
        <button
          class={iconBtnCls}
          onclick={regenerateBranchName}
          aria-label="Regenerate from template"
          title="Regenerate the name from the template"
        >
          <RotateCcw size={14} />
        </button>
      {/if}
    </div>
    <span class="sr-only" role="status" aria-live="polite">{operationError || operationStatus}</span
    >
    {#if operationStatus || operationError}
      <div
        class="rounded-lg border px-3 py-2 text-xs leading-snug"
        class:border-danger={operationError}
        class:border-border={!operationError}
        class:bg-danger-bg={operationError}
        class:bg-bg-input={!operationError}
        class:text-danger-text={operationError}
        class:text-text-muted={!operationError}
      >
        {#if operationError}
          <p class="m-0 font-medium">{operationError}</p>
          {#if createdWorktreePath}
            <p class="mt-1 mb-0">
              The worktree was created. Start or focus an agent there, then use Retry Send.
            </p>
          {/if}
        {:else}
          {operationStatus}
        {/if}
      </div>
    {/if}
    {#if availableAgents.length > 0}
      <div class="flex items-center gap-2.5">
        <span class="text-sm text-text-muted w-[50px] flex-shrink-0">Agent</span>
        <CustomSelect
          value={selectedAgentId}
          options={[
            { value: '', label: 'None' },
            ...availableAgents.map((t) => ({ value: t.id, label: t.name })),
          ]}
          onchange={(v) => {
            selectedAgentId = v
          }}
          maxWidth="none"
        />
      </div>
    {/if}
    <div class="flex justify-end gap-2 pt-1 border-t border-border-subtle">
      <button
        class="px-3.5 py-1.5 border-0 rounded-lg bg-active text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover-strong"
        onclick={onBack}>Back</button
      >
      <button
        class="px-3.5 py-1.5 border-0 rounded-lg bg-accent-bg text-accent-text text-sm font-inherit cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={confirmBranchCreation}
        disabled={creatingWorktree || !resolvedBranchName || !selectedBaseBranch}
      >
        {#if creatingWorktree}
          Working...
        {:else if createdWorktreePath && contextSendFailed}
          {selectedAgentId ? 'Retry Send' : 'Select Agent to Retry'}
        {:else if selectedAgentId}
          Create & Start Agent
        {:else}
          Create & Switch
        {/if}
      </button>
    </div>
  </div>
{/if}
