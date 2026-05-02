<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { X, ExternalLink, ArrowLeft } from '@lucide/svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
  import { getPref, setPref } from '../../lib/stores/preferences.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { workspaceState, selectWorktree } from '../../lib/stores/workspace.svelte'
  import { getTools, getToolAvailability } from '../../lib/stores/tools.svelte'
  import { isAiToolId, openTool } from '../../lib/stores/tabs.svelte'
  import { agentSessions } from '../../lib/agents/agentState.svelte'
  import { fetchAndFormatTaskContext } from '../../lib/taskTracker/taskContext'
  import { setActiveTask } from '../../lib/stores/taskTracker.svelte'
  import { safeDirName } from '../../lib/sanitize'
  import { wrapAsBracketedPaste } from '../../lib/pty/paste'

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
  let creatingWorktree = $state(false)
  let templateHasBranchType = $state(false)
  let initialized = $state(false)
  let fullTask = $state<Task>(untrack(() => task))
  let selectedAgentId = $state(getPref('taskTracker.lastAgent', ''))

  let availableAgents = $derived.by(() => {
    const tools = getTools()
    const avail = getToolAvailability()
    return tools.filter((t) => isAiToolId(t.id) && avail[t.id])
  })

  async function init(): Promise<void> {
    const [typeInfo, foundTask] = await Promise.all([
      window.api
        .taskTrackerResolveBranchType(
          task.type,
          connectionId,
          selectedBoardId || undefined,
          workspaceState.repoRoot || undefined,
        )
        .catch(() => null),
      window.api.taskTrackerFindTaskByKey(task.key).catch(() => null),
    ])

    if (foundTask) fullTask = foundTask as Task

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
    try {
      const plain = JSON.parse(JSON.stringify(task)) as Task
      resolvedBranchName = await window.api.taskTrackerResolveBranchName(
        connectionId,
        plain,
        selectedBoardId || undefined,
        templateHasBranchType ? selectedBranchType : undefined,
        workspaceState.repoRoot || undefined,
      )
    } catch {
      resolvedBranchName = task.key
    }
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
    const repoRoot = workspaceState.repoRoot
    const currentBranch = workspaceState.branch
    if (!repoRoot || !currentBranch || !resolvedBranchName) return

    const baseDir = getPref('worktrees.baseDir', '~/canopy/worktrees')
    const projectName = repoRoot.split(/[/\\]/).pop() || 'project'
    const safeBranchName = safeDirName(resolvedBranchName)
    const worktreeDir = `${baseDir}/${projectName}/${safeBranchName}`
    const homedir = await window.api.getHomedir()
    const worktreePath = worktreeDir.startsWith('~/')
      ? (homedir + worktreeDir.slice(1)).replace(/\\/g, '/')
      : worktreeDir

    creatingWorktree = true
    setPref('taskTracker.lastAgent', selectedAgentId)
    try {
      await window.api.gitWorktreeAdd(repoRoot, worktreePath, resolvedBranchName, currentBranch)
      await setActiveTask(worktreePath, {
        taskKey: fullTask.key,
        summary: fullTask.summary,
        connectionId,
        boardId: selectedBoardId || undefined,
      })

      if (hasSetupConfig()) {
        const wsId = workspaceState.workspace!.id
        addToast('Running worktree setup...')
        try {
          await window.api.runWorktreeSetup(wsId, repoRoot, worktreePath)
          addToast('Worktree setup complete')
        } catch (e) {
          addToast('Worktree setup failed: ' + (e instanceof Error ? e.message : String(e)))
        }
      }

      if (selectedAgentId) {
        const agentId = selectedAgentId
        const connId = connectionId
        const taskSnapshot = JSON.parse(JSON.stringify(fullTask)) as typeof fullTask

        try {
          const tab = await openTool(agentId, worktreePath)
          await selectWorktree(worktreePath)
          const pane = tab.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
          if (pane) {
            const sessionId = pane.sessionId
            const ready = await waitForAgentIdle(sessionId)
            if (ready) {
              const context = await fetchAndFormatTaskContext(
                connId,
                taskSnapshot,
                workspaceState.repoRoot ?? undefined,
              )
              await window.api.writePty(sessionId, wrapAsBracketedPaste(context) + '\r')
            }
          }
        } catch {
          addToast('Failed to send task context to agent')
        }
      } else {
        await selectWorktree(worktreePath)
      }

      closeDialog()
    } catch (e) {
      creatingWorktree = false
      closeDialog()
      await new Promise((r) => setTimeout(r, 0))
      await confirm({
        title: 'Worktree Creation Failed',
        message: e instanceof Error ? e.message : 'Failed to create worktree',
        confirmLabel: 'OK',
      })
    }
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
    <div class="flex items-center gap-2.5">
      <span class="text-sm text-text-muted w-[50px] flex-shrink-0">Branch</span>
      <code class="text-sm text-accent-text bg-bg-input px-2.5 py-[5px] rounded-lg flex-1"
        >{resolvedBranchName}</code
      >
    </div>
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
        disabled={creatingWorktree || !resolvedBranchName}
      >
        {#if creatingWorktree}Creating...{:else if selectedAgentId}Create & Start Agent{:else}Create
          & Switch{/if}
      </button>
    </div>
  </div>
{/if}
