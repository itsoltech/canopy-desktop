<script lang="ts">
  import { AlertTriangle, GitPullRequest, LoaderCircle } from '@lucide/svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { showCreateTaskPR, showPRDetails } from '../../lib/stores/dialogs.svelte'
  import {
    getPRFallbackGeneration,
    getPRForBranch,
    invalidatePRFallback,
    loadPRFallbackSummary,
  } from '../../lib/stores/github.svelte'
  import { getPanelTask, getPanelTaskResolvedPath } from '../../lib/stores/taskTracker.svelte'
  import { prStateChip } from '../../lib/github/prState'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'

  let { separator = false }: { separator?: boolean } = $props()

  function worktreePath(): string {
    return workspaceState.selectedWorktreePath ?? workspaceState.repoRoot!
  }

  let branchPR = $derived(
    workspaceState.branch
      ? getPRForBranch(workspaceState.repoRoot, workspaceState.branch)
      : undefined,
  )
  let fallbackPR = $state<{ number: number; state: string; isDraft: boolean } | null>(null)
  let loading = $state(false)
  let lookupError = $state('')

  $effect(() => {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    const branch = workspaceState.branch
    void getPRFallbackGeneration(path, branch)
    fallbackPR = null
    lookupError = ''
    if (!path || !branch || branchPR) {
      loading = false
      return
    }

    let cancelled = false
    loading = true
    loadPRFallbackSummary(path, branch)
      .then((pr) => {
        if (!cancelled && pr) {
          fallbackPR = { number: pr.number, state: pr.state, isDraft: pr.isDraft }
        }
      })
      .catch((error) => {
        if (!cancelled) lookupError = ipcErrorMessage(error, 'Failed to check pull requests')
      })
      .finally(() => {
        if (!cancelled) loading = false
      })

    return () => {
      cancelled = true
    }
  })

  let existingPR = $derived.by(() => {
    if (branchPR) {
      return { number: branchPR.number, state: branchPR.state, isDraft: branchPR.isDraft }
    }
    return fallbackPR
  })
  let showCreatePR = $derived(!lookupError && (!existingPR || existingPR.state !== 'OPEN'))

  function retryLookup(): void {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    const branch = workspaceState.branch
    if (path && branch) invalidatePRFallback(path, branch)
  }

  function openExistingPR(): void {
    if (workspaceState.branch) showPRDetails(worktreePath(), workspaceState.branch)
  }

  function createPR(): void {
    if (!workspaceState.branch) return
    const resolvedFor = getPanelTaskResolvedPath()
    const task = resolvedFor === worktreePath().replace(/\\/g, '/') ? getPanelTask() : null
    showCreateTaskPR(
      worktreePath(),
      workspaceState.branch,
      task
        ? {
            taskKey: task.taskKey,
            summary: task.summary,
            connectionId: task.connectionId || undefined,
          }
        : undefined,
    )
  }
</script>

<span class="sr-only" aria-live="polite">{loading ? 'Checking pull requests...' : lookupError}</span
>

{#if separator}
  <div class="h-px mx-3 my-1 bg-border-subtle" role="separator" aria-orientation="horizontal"></div>
{/if}

{#if loading && !existingPR}
  <div class="flex items-center gap-2.5 w-full h-7 px-3 text-sm text-text-faint">
    <LoaderCircle size={13} class="animate-spin-slow flex-shrink-0 motion-reduce:animate-none" />
    <span class="flex-1">Checking pull requests...</span>
  </div>
{/if}

{#if lookupError && !loading && !existingPR}
  <div class="flex items-center gap-2 w-full min-h-7 px-3 text-xs text-warning-text">
    <AlertTriangle size={13} class="flex-shrink-0" />
    <span class="flex-1 min-w-0 break-words">{lookupError}</span>
    <button
      type="button"
      class="flex-shrink-0 border-0 rounded-sm bg-transparent px-1 py-0.5 text-xs text-text-secondary cursor-pointer hover:bg-hover hover:text-text"
      onclick={retryLookup}
      title="Retry pull request lookup">Retry</button
    >
  </div>
{/if}

{#if existingPR}
  {@const chip = prStateChip(existingPR.state, existingPR.isDraft)}
  <button
    class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
    onclick={openExistingPR}
    title={branchPR
      ? `View PR #${branchPR.number} - ${branchPR.title}`
      : 'View the latest pull request for this branch'}
  >
    <GitPullRequest
      size={13}
      class="text-text-faint group-enabled:group-hover:text-accent-text flex-shrink-0"
    />
    <span class="flex-1">View PR #{existingPR.number}</span>
    {#if chip.label}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 {chip.cls}">{chip.label}</span>
    {/if}
  </button>
{/if}

{#if showCreatePR && !loading}
  <button
    class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
    disabled={!workspaceState.branch}
    onclick={createPR}
    title="Create a pull request from this branch - edit the title and description before it is created"
  >
    <GitPullRequest
      size={13}
      class="text-text-faint group-enabled:group-hover:text-accent-text flex-shrink-0"
    />
    <span class="flex-1">Create PR</span>
  </button>
{/if}
