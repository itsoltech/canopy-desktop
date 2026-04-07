<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
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
  let fullTask = $state<Task>(task)
  let selectedAgentId = $state(getPref('taskTracker.lastAgent', ''))

  let availableAgents = $derived.by(() => {
    const tools = getTools()
    const avail = getToolAvailability()
    return tools.filter((t) => isAiToolId(t.id) && avail[t.id])
  })

  async function init(): Promise<void> {
    // Fetch full task data (with description) in parallel with branch type
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
    const safeBranchName = resolvedBranchName.replace(/\//g, '-')
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
      closeDialog()

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
        // Capture values before component is destroyed by closeDialog
        const agentId = selectedAgentId
        const connId = connectionId
        const taskSnapshot = JSON.parse(JSON.stringify(fullTask)) as typeof fullTask

        // Open agent tab BEFORE switching worktree so ensureDefaultTab
        // sees an existing tab and doesn't race with a shell tab
        try {
          const tab = await openTool(agentId, worktreePath)
          await selectWorktree(worktreePath)
          const pane = tab.rootSplit.type === 'leaf' ? tab.rootSplit.pane : null
          if (pane) {
            const sessionId = pane.sessionId
            const ready = await waitForAgentIdle(sessionId)
            if (ready) {
              const context = await fetchAndFormatTaskContext(connId, taskSnapshot)
              await window.api.writePty(sessionId, context + '\n')
            }
          }
        } catch {
          addToast('Failed to send task context to agent')
        }
      } else {
        await selectWorktree(worktreePath)
      }
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

  onDestroy(() => {
    window.api.abortWorktreeSetup()
  })
</script>

{#if initialized}
  <div class="picker-header">
    <button class="back-btn" onclick={onBack} aria-label="Back">
      <ArrowLeft size={16} />
    </button>
    <h3 class="picker-title">Create Branch</h3>
    <button class="close-btn" onclick={() => closeDialog()} aria-label="Close">
      <X size={16} />
    </button>
  </div>
  <div class="branch-form">
    <div class="task-card">
      <div class="task-card-header">
        {#if fullTask.url}
          <button class="task-key-link" onclick={() => window.api.openExternal(fullTask.url!)}>
            {fullTask.key}
            <ExternalLink size={11} />
          </button>
        {:else}
          <span class="task-key">{fullTask.key}</span>
        {/if}
        {#if fullTask.status}
          <span class="task-status">{fullTask.status}</span>
        {/if}
      </div>
      <p class="task-title">{fullTask.summary}</p>
      {#if fullTask.description}
        <p class="task-description">{fullTask.description}</p>
      {/if}
    </div>

    {#if templateHasBranchType}
      <div class="field-row">
        <span class="field-label">Type</span>
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
    <div class="field-row">
      <span class="field-label">Branch</span>
      <code class="branch-preview">{resolvedBranchName}</code>
    </div>
    {#if availableAgents.length > 0}
      <div class="field-row">
        <span class="field-label">Agent</span>
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
    <div class="branch-actions">
      <button class="btn-cancel" onclick={onBack}>Back</button>
      <button
        class="btn-create"
        onclick={confirmBranchCreation}
        disabled={creatingWorktree || !resolvedBranchName}
      >
        {#if creatingWorktree}Creating...{:else if selectedAgentId}Create & Start Agent{:else}Create
          & Switch{/if}
      </button>
    </div>
  </div>
{/if}

<style>
  .picker-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 16px 10px;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .picker-title {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
    flex: 1;
  }

  .back-btn,
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
    flex-shrink: 0;
  }

  .back-btn:hover,
  .close-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .branch-form {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .task-card {
    padding: 10px 12px;
    background: var(--c-bg-input);
    border: 1px solid var(--c-border);
    border-radius: 8px;
  }

  .task-card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  .task-key {
    font-weight: 600;
    font-size: 12px;
    color: var(--c-accent-text);
  }

  .task-key-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
    font-size: 12px;
    color: var(--c-accent-text);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: inherit;
  }

  .task-key-link:hover {
    text-decoration: underline;
  }

  .task-status {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--c-active);
    color: var(--c-text-muted);
  }

  .task-title {
    margin: 0;
    font-size: 13px;
    color: var(--c-text);
    line-height: 1.4;
  }

  .task-description {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.4;
    max-height: 60px;
    overflow-y: auto;
  }

  .field-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .field-label {
    font-size: 12px;
    color: var(--c-text-muted);
    width: 50px;
    flex-shrink: 0;
  }

  .branch-preview {
    font-size: 12px;
    color: var(--c-accent-text);
    background: var(--c-bg-input);
    padding: 5px 10px;
    border-radius: 6px;
    flex: 1;
  }

  .branch-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 4px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .btn-cancel {
    padding: 6px 14px;
    border: none;
    border-radius: 6px;
    background: var(--c-active);
    color: var(--c-text-secondary);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-cancel:hover {
    background: var(--c-hover-strong);
  }

  .btn-create {
    padding: 6px 14px;
    border: none;
    border-radius: 6px;
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-create:hover:not(:disabled) {
    background: var(--c-accent-bg-hover);
  }

  .btn-create:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
