<script lang="ts">
  import { tick } from 'svelte'
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
  import { pendingPRLookup, settledPRLookup } from '../../lib/github/prLookupState'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import Tooltip from '../shared/Tooltip.svelte'

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
  let lookup = $state({ loading: false, error: '' })
  let createPRButtonEl: HTMLButtonElement | undefined = $state()
  let viewPRButtonEl: HTMLButtonElement | undefined = $state()
  let retryButtonEl: HTMLButtonElement | undefined = $state()

  $effect(() => {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    const branch = workspaceState.branch
    void getPRFallbackGeneration(path, branch)
    fallbackPR = null
    lookup = settledPRLookup()
    if (!path || !branch || branchPR) {
      return
    }

    let cancelled = false
    lookup = pendingPRLookup()
    loadPRFallbackSummary(path, branch)
      .then((pr) => {
        if (!cancelled && pr) {
          fallbackPR = { number: pr.number, state: pr.state, isDraft: pr.isDraft }
        }
        if (!cancelled) lookup = settledPRLookup()
      })
      .catch((error) => {
        if (!cancelled) {
          lookup = settledPRLookup(ipcErrorMessage(error, 'Failed to check pull requests'))
        }
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
  let showCreatePR = $derived(!lookup.error && (!existingPR || existingPR.state !== 'OPEN'))
  let createPRBlockedReason = $derived(
    lookup.loading
      ? 'Checking pull requests...'
      : workspaceState.branch
        ? ''
        : 'No branch checked out',
  )
  let createPRTitle = $derived(
    createPRBlockedReason ||
      'Create a pull request from this branch - edit the title and description before it is created',
  )

  $effect.pre(() => {
    // Keep keyboard position when the lookup changes the available action. Create remains
    // mounted while loading; final View/Create/Retry replacements transfer focus after Svelte
    // updates the DOM instead of dropping it onto document.body.
    const active = document.activeElement
    if (!showCreatePR && active === createPRButtonEl) {
      void tick().then(() => (viewPRButtonEl ?? retryButtonEl)?.focus())
    } else if (!existingPR && active === viewPRButtonEl) {
      void tick().then(() => (createPRButtonEl ?? retryButtonEl)?.focus())
    } else if ((!lookup.error || lookup.loading || existingPR) && active === retryButtonEl) {
      void tick().then(() => (createPRButtonEl ?? viewPRButtonEl)?.focus())
    }
  })

  function retryLookup(): void {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    const branch = workspaceState.branch
    if (path && branch) invalidatePRFallback(path, branch)
  }

  function openExistingPR(): void {
    if (workspaceState.branch) showPRDetails(worktreePath(), workspaceState.branch)
  }

  function createPR(): void {
    if (!workspaceState.branch || lookup.loading) return
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

<span class="sr-only" aria-live="polite"
  >{lookup.loading ? 'Checking pull requests...' : lookup.error}</span
>

{#if separator}
  <div class="h-px mx-3 my-1 bg-border-subtle" role="separator" aria-orientation="horizontal"></div>
{/if}

{#if lookup.error && !lookup.loading && !existingPR}
  <div class="flex items-center gap-2 w-full min-h-7 px-3 text-xs text-warning-text">
    <AlertTriangle size={13} class="flex-shrink-0" />
    <span class="flex-1 min-w-0 break-words">{lookup.error}</span>
    <button
      bind:this={retryButtonEl}
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
    bind:this={viewPRButtonEl}
    class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
    onclick={openExistingPR}
    title={branchPR
      ? `View PR #${branchPR.number} - ${branchPR.title}`
      : 'View the latest pull request for this branch'}
  >
    <GitPullRequest
      size={13}
      class="text-text-faint group-hover:text-accent-text group-focus-within:text-accent-text flex-shrink-0"
    />
    <span class="flex-1">View PR #{existingPR.number}</span>
    {#if chip.label}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 {chip.cls}">{chip.label}</span>
    {/if}
  </button>
{/if}

{#if showCreatePR}
  <Tooltip text={createPRTitle} class="w-full">
    <button
      bind:this={createPRButtonEl}
      class="{createPRBlockedReason
        ? ''
        : 'group '}flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
      aria-disabled={!!createPRBlockedReason}
      aria-busy={lookup.loading}
      aria-describedby={createPRBlockedReason ? 'create-pr-blocked-reason' : undefined}
      onclick={createPR}
    >
      {#if lookup.loading}
        <LoaderCircle
          size={13}
          class="animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
        />
        <span class="flex-1">Checking pull requests...</span>
      {:else}
        <GitPullRequest
          size={13}
          class="text-text-faint group-hover:text-accent-text group-focus-within:text-accent-text flex-shrink-0"
        />
        <span class="flex-1">Create PR</span>
      {/if}
    </button>
  </Tooltip>
  {#if createPRBlockedReason}
    <span id="create-pr-blocked-reason" class="sr-only">{createPRBlockedReason}</span>
  {/if}
{/if}
