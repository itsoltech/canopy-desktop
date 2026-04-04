<script lang="ts">
  import { onMount } from 'svelte'
  import { SquareKanban, Plus, ExternalLink, Settings, GitPullRequest } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import {
    getRepoConfig,
    getHasCredentials,
    isTaskTrackerLoading,
    getTaskTrackerConnections,
    getActiveTask,
    loadConnections,
  } from '../../lib/stores/taskTracker.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { showPreferences, confirm } from '../../lib/stores/dialogs.svelte'
  import { showTaskPicker } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'

  onMount(() => {
    loadConnections()
  })

  let config = $derived(getRepoConfig())
  let hasCreds = $derived(getHasCredentials())
  let loading = $derived(isTaskTrackerLoading())
  let connections = $derived(getTaskTrackerConnections())
  let activeTask = $derived(getActiveTask())
  let creatingPR = $state(false)

  // Fallback: extract task key from branch name if no active task stored
  let taskKeyFromBranch = $derived.by(() => {
    if (activeTask) return null
    const branch = workspaceState.branch
    if (!branch) return null
    const match = branch.match(/([A-Z][A-Z0-9]+-\d+)/)
    return match ? match[1] : null
  })
  let canCreatePR = $derived(!!(activeTask || taskKeyFromBranch) && !!workspaceState.branch)

  function providerLabel(provider: string): string {
    if (provider === 'jira') return 'Jira'
    if (provider === 'youtrack') return 'YouTrack'
    return provider
  }

  function openTrackerPrefs(): void {
    showPreferences('tasks')
  }

  function browseTasks(connectionId: string): void {
    showTaskPicker(connectionId)
  }

  function worktreePath(): string {
    return workspaceState.selectedWorktreePath ?? workspaceState.repoRoot ?? ''
  }

  async function doCreatePR(): Promise<void> {
    const branch = workspaceState.branch
    if (!branch || !canCreatePR) return

    const taskKey = activeTask?.taskKey ?? taskKeyFromBranch ?? ''
    creatingPR = true
    let prTitle = `[${taskKey}]`
    let defaultTarget = 'develop'
    try {
      const preview = await window.api.taskTrackerResolvePRPreview(
        taskKey,
        activeTask?.connectionId,
        activeTask?.boardId,
      )
      prTitle = preview.title
      defaultTarget = preview.targetBranch
    } catch {
      // use defaults
    }
    creatingPR = false

    const ok = await confirm({
      title: 'Create Pull Request',
      message: `Create PR from "${workspaceState.branch}"?`,
      details: `Title: ${prTitle}\nTarget: ${defaultTarget}`,
      confirmLabel: 'Create PR',
    })
    if (!ok) return

    creatingPR = true
    try {
      const result = await window.api.taskTrackerCreatePR(
        worktreePath(),
        {
          key: taskKey,
          summary: activeTask?.summary ?? '',
          description: '',
          status: '',
          priority: '',
          type: 'task',
        },
        branch,
        activeTask?.connectionId,
        activeTask?.boardId,
      )
      addToast('PR created')
      window.api.openExternal(result.url)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      if (msg.includes('No commits between')) {
        await confirm({
          title: 'No Changes',
          message: `No commits between target branch and "${workspaceState.branch}". Commit changes first.`,
          confirmLabel: 'OK',
        })
      } else {
        addToast(msg)
      }
    } finally {
      creatingPR = false
    }
  }
</script>

<CollapsibleSection title="TASKS" sectionKey="tasks" borderTop>
  {#if loading}
    <div class="loading">Loading...</div>
  {:else if config}
    <ul class="tracker-list">
      <li>
        <button
          class="tracker-item"
          onclick={() => {
            if (connections.length > 0) {
              browseTasks(connections[0].id)
            }
          }}
          disabled={!hasCreds || connections.length === 0}
          title={hasCreds
            ? `Browse tasks — ${providerLabel(config.tracker.provider)}`
            : 'Credentials required'}
        >
          <SquareKanban size={14} />
          <span class="tracker-name">{config.tracker.baseUrl || 'Not configured'}</span>
          <span class="tracker-provider">{providerLabel(config.tracker.provider)}</span>
          {#if hasCreds}
            <ExternalLink size={12} />
          {/if}
        </button>
      </li>
    </ul>

    {#if activeTask}
      <div class="active-task">
        <span class="task-key">{activeTask.taskKey}</span>
        <span class="task-summary">{activeTask.summary}</span>
      </div>
    {/if}

    {#if canCreatePR}
      <div class="pr-row">
        <button
          class="pr-btn"
          onclick={doCreatePR}
          disabled={creatingPR}
          title="Create Pull Request for {activeTask?.taskKey ?? taskKeyFromBranch}"
        >
          <GitPullRequest size={13} />
          <span>Create PR</span>
          <span class="pr-task-key">{activeTask?.taskKey ?? taskKeyFromBranch}</span>
        </button>
      </div>
    {/if}

    {#if !hasCreds}
      <div class="token-hint">
        <button class="connect-btn" onclick={openTrackerPrefs}>
          Credentials required — configure in Preferences
        </button>
      </div>
    {/if}
    <div class="add-row">
      <button class="add-btn" onclick={openTrackerPrefs} title="Configure tracker">
        <Settings size={12} />
        <span>Settings</span>
      </button>
    </div>
  {:else}
    <div class="empty-state">
      <button class="connect-btn" onclick={openTrackerPrefs}>
        <Plus size={14} />
        Configure Tracker
      </button>
    </div>
  {/if}
</CollapsibleSection>

<style>
  .loading {
    padding: 8px 12px;
    font-size: 11px;
    color: var(--c-text-muted);
  }

  .empty-state {
    padding: 8px 12px;
  }

  .token-hint {
    padding: 4px 12px;
  }

  .connect-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 10px;
    border: 1px dashed var(--c-border);
    border-radius: 6px;
    background: none;
    color: var(--c-text-muted);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      border-color 0.1s,
      color 0.1s;
  }

  .connect-btn:hover {
    border-color: var(--c-accent-muted);
    color: var(--c-accent-text);
  }

  .tracker-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .tracker-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 12px;
    border: none;
    background: none;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .tracker-item:hover:not(:disabled) {
    background: var(--c-hover);
  }

  .tracker-item:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .tracker-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .tracker-provider {
    font-size: 10px;
    color: var(--c-text-faint);
    flex-shrink: 0;
  }

  .active-task {
    padding: 6px 12px;
    display: flex;
    align-items: center;
    gap: 8px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .task-key {
    font-size: 11px;
    font-weight: 600;
    color: var(--c-accent-text);
    flex-shrink: 0;
  }

  .task-summary {
    font-size: 11px;
    color: var(--c-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .pr-row {
    padding: 4px 12px;
  }

  .pr-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 8px;
    border: none;
    border-radius: 4px;
    background: var(--c-active);
    color: var(--c-text-secondary);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.1s;
  }

  .pr-task-key {
    margin-left: auto;
    font-size: 10px;
    color: var(--c-text-faint);
  }

  .pr-btn:hover:not(:disabled) {
    background: var(--c-hover-strong);
    color: var(--c-text);
  }

  .pr-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .add-row {
    padding: 4px 12px 4px;
  }

  .add-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border: none;
    background: none;
    color: var(--c-text-faint);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: color 0.1s;
  }

  .add-btn:hover {
    color: var(--c-text-secondary);
  }
</style>
