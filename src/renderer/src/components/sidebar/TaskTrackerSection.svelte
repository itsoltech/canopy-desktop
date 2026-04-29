<script lang="ts">
  import { SquareKanban, Plus, ExternalLink, Settings, GitPullRequest } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import {
    getResolvedConfig,
    getTrackerCredentials,
    isTaskTrackerLoading,
    getActiveTask,
  } from '../../lib/stores/taskTracker.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { showPreferences, confirm } from '../../lib/stores/dialogs.svelte'
  import { showTaskPicker } from '../../lib/stores/dialogs.svelte'
  import { providerLabel } from '../../lib/taskTracker/providerLabel'
  import { addToast } from '../../lib/stores/toast.svelte'

  let resolved = $derived(getResolvedConfig())
  let trackers = $derived(resolved?.config.trackers ?? [])
  let trackerCreds = $derived(getTrackerCredentials())
  let loading = $derived(isTaskTrackerLoading())
  let activeTask = $derived(getActiveTask())
  let creatingPR = $state(false)

  let taskKeyFromBranch = $derived.by(() => {
    if (activeTask) return null
    const branch = workspaceState.branch
    if (!branch) return null
    const match = branch.match(/([A-Z][A-Z0-9]+-\d+)/)
    return match ? match[1] : null
  })
  let canCreatePR = $derived(!!(activeTask || taskKeyFromBranch) && !!workspaceState.branch)
  let existingPRUrl = $state<string | null>(null)

  let prCheckTimer: ReturnType<typeof setTimeout> | null = null

  $effect(() => {
    const branch = workspaceState.branch
    existingPRUrl = null
    if (prCheckTimer) clearTimeout(prCheckTimer)
    if (branch) {
      prCheckTimer = setTimeout(() => checkExistingPR(branch), 500)
    }
    return () => {
      if (prCheckTimer) clearTimeout(prCheckTimer)
    }
  })

  async function checkExistingPR(branch: string): Promise<void> {
    const root = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    if (!root) return
    try {
      const result = await window.api.taskTrackerFindPR(root, branch)
      if (result) existingPRUrl = result
    } catch {
      // no PR found
    }
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
        worktreePath() || undefined,
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
      existingPRUrl = result.url
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
    <div class="px-3 py-2 text-xs text-text-muted">Loading...</div>
  {:else if trackers.length > 0}
    <ul class="list-none p-0 m-0">
      {#each trackers as tracker (tracker.id)}
        {@const hasCreds = trackerCreds[tracker.id]?.hasToken ?? false}
        <li>
          <button
            class="flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-default"
            onclick={() => browseTasks(tracker.id)}
            disabled={!hasCreds}
            title={hasCreds
              ? `Browse tasks — ${providerLabel(tracker.provider)}`
              : 'Credentials required'}
          >
            <SquareKanban size={14} />
            <span
              class="overflow-hidden text-ellipsis whitespace-nowrap flex-1"
              title={tracker.baseUrl || 'Not configured'}
              >{tracker.baseUrl || 'Not configured'}</span
            >
            <span
              class="inline-flex items-center h-4 px-1.5 rounded-md text-2xs font-semibold uppercase tracking-caps-tight bg-border-subtle text-text-muted leading-tight flex-shrink-0"
              >{providerLabel(tracker.provider)}</span
            >
            {#if hasCreds}
              <ExternalLink size={12} />
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    {#if activeTask}
      <div
        class="flex items-center gap-2 mx-2 mt-1 px-3 py-1.5 rounded-md bg-bg-elevated border border-border-subtle"
      >
        <span class="text-xs font-semibold text-accent-text flex-shrink-0"
          >{activeTask.taskKey}</span
        >
        <span
          class="text-xs text-text-muted overflow-hidden text-ellipsis whitespace-nowrap flex-1"
          title={activeTask.summary}>{activeTask.summary}</span
        >
      </div>
    {/if}

    <div class="px-3 py-1">
      {#if existingPRUrl}
        <button
          class="flex items-center gap-1.5 w-full px-2 py-1 border-0 rounded-md bg-active text-text-secondary text-xs font-inherit cursor-pointer transition-colors duration-fast enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
          onclick={() => window.api.openExternal(existingPRUrl!)}
          title="Open existing Pull Request"
        >
          <ExternalLink size={13} />
          <span>Open PR</span>
          <span class="ml-auto text-2xs text-text-faint"
            >{activeTask?.taskKey ?? taskKeyFromBranch}</span
          >
        </button>
      {:else}
        <button
          class="flex items-center gap-1.5 w-full px-2 py-1 border-0 rounded-md bg-active text-text-secondary text-xs font-inherit cursor-pointer transition-colors duration-fast enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
          onclick={doCreatePR}
          disabled={creatingPR || !canCreatePR}
          title={canCreatePR
            ? `Create Pull Request for ${activeTask?.taskKey ?? taskKeyFromBranch}`
            : 'No task key found in branch name'}
        >
          <GitPullRequest size={13} />
          <span>{creatingPR ? 'Creating...' : 'Create PR'}</span>
          {#if activeTask?.taskKey ?? taskKeyFromBranch}
            <span class="ml-auto text-2xs text-text-faint"
              >{activeTask?.taskKey ?? taskKeyFromBranch}</span
            >
          {/if}
        </button>
      {/if}
    </div>

    {#if trackers.some((t) => !trackerCreds[t.id]?.hasToken)}
      <div class="px-3 py-1">
        <button
          class="flex items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
          onclick={openTrackerPrefs}
        >
          Credentials required — configure in Preferences
        </button>
      </div>
    {/if}
    <div class="px-3 py-1">
      <button
        class="flex items-center gap-1 px-1.5 py-0.5 border-0 bg-transparent text-text-faint text-xs font-inherit cursor-pointer transition-colors duration-fast hover:text-text-secondary"
        onclick={openTrackerPrefs}
        title="Configure tracker"
      >
        <Settings size={12} />
        <span>Settings</span>
      </button>
    </div>
  {:else}
    <div class="px-3 py-2">
      <button
        class="flex items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
        onclick={openTrackerPrefs}
      >
        <Plus size={14} />
        Configure Tracker
      </button>
    </div>
  {/if}
</CollapsibleSection>
