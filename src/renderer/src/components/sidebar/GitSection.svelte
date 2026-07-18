<script lang="ts">
  import {
    GitCommitVertical,
    ArrowUpFromLine,
    ArrowDownToLine,
    RefreshCw,
    Archive,
    ArchiveRestore,
    GitPullRequest,
    LoaderCircle,
  } from '@lucide/svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { confirm, prompt, showPRDetails, showCreateTaskPR } from '../../lib/stores/dialogs.svelte'
  import { getPRForBranch, getPRRefreshTick } from '../../lib/stores/github.svelte'
  import { getPanelTask } from '../../lib/stores/taskTracker.svelte'
  import { prStateChip } from '../../lib/github/prState'
  import CollapsibleSection from './CollapsibleSection.svelte'

  let loading: string | null = $state(null)

  function worktreePath(): string {
    return workspaceState.selectedWorktreePath ?? workspaceState.repoRoot!
  }

  async function gitError(err: unknown): Promise<void> {
    await confirm({
      title: 'Git Error',
      message: err instanceof Error ? err.message : String(err),
      confirmLabel: 'OK',
    })
  }

  async function doCommit(): Promise<void> {
    const result = await prompt({
      title: 'Commit',
      placeholder: 'Commit message...',
      multiline: true,
      submitLabel: 'Commit',
      onGenerate: () => window.api.gitGenerateCommitMessage(worktreePath()),
      checkbox: { label: 'Stage all changes', checked: true },
    })
    if (!result) return
    loading = 'commit'
    try {
      await window.api.gitCommitWorktree({
        repoRoot: worktreePath(),
        message: result.value,
        stageAll: result.checked,
      })
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doPush(): Promise<void> {
    loading = 'push'
    try {
      const root = worktreePath()
      const preflight = await window.api.gitPreparePush({ repoRoot: root })
      const ok = await confirm({
        title: 'Push',
        message: preflight.confirmationMessage,
      })
      if (ok) {
        await window.api.gitPushWorktree({ repoRoot: root })
      }
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doPull(): Promise<void> {
    loading = 'pull'
    try {
      await window.api.gitPullWithPreferences({ repoRoot: worktreePath() })
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doFetch(): Promise<void> {
    loading = 'fetch'
    try {
      await window.api.gitFetchWorktree({ repoRoot: worktreePath() })
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doStash(): Promise<void> {
    loading = 'stash'
    try {
      await window.api.gitStashWorktree({ repoRoot: worktreePath() })
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doStashPop(): Promise<void> {
    loading = 'stashPop'
    try {
      await window.api.gitStashPopWorktree({ repoRoot: worktreePath() })
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  let ahead = $derived(workspaceState.aheadBehind?.ahead ?? 0)
  let behind = $derived(workspaceState.aheadBehind?.behind ?? 0)

  // --- Pull requests for the current branch (moved here from PROJECT MANAGEMENT — a PR belongs
  // to the branch, and this works without any tracker configured).
  let branchPR = $derived(workspaceState.branch ? getPRForBranch(workspaceState.branch) : undefined)
  // The github store map needs the GitHub API integration; fall back to the gh CLI (same auth as
  // PR creation) so the "View PR" row appears even without it — with the PR state for a chip.
  let fallbackPR = $state<{ number: number; state: string; isDraft: boolean } | null>(null)
  $effect(() => {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    const branch = workspaceState.branch
    // Re-check after any PR mutation elsewhere in the app (create/merge/close bump the tick).
    void getPRRefreshTick()
    fallbackPR = null
    if (!path || !branch || branchPR) return
    let cancelled = false
    window.api
      .taskTrackerPRDetails(path, branch)
      .then((pr) => {
        if (!cancelled && pr) {
          fallbackPR = { number: pr.number, state: pr.state, isDraft: pr.isDraft }
        }
      })
      .catch(() => {})
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
  // A branch can accumulate merged/closed PRs — only an ACTIVE (open) one blocks a new PR.
  let showCreatePRRow = $derived(!existingPR || existingPR.state !== 'OPEN')

  function openExistingPR(): void {
    if (!workspaceState.branch) return
    showPRDetails(worktreePath(), workspaceState.branch)
  }

  function doCreatePR(): void {
    if (!workspaceState.branch) return
    // The linked tracker task (when there is one) provides the PR template context; without it
    // the form falls back to a plain branch-level PR.
    const t = getPanelTask()
    showCreateTaskPR(
      worktreePath(),
      workspaceState.branch,
      t
        ? { taskKey: t.taskKey, summary: t.summary, connectionId: t.connectionId || undefined }
        : undefined,
    )
  }
</script>

<span class="sr-only" aria-live="polite">{loading ? `${loading} in progress…` : ''}</span>
<CollapsibleSection title="GIT" sectionKey="git" borderTop>
  {#snippet headerExtra()}
    <span class="flex items-center gap-1 min-w-0">
      <span class="text-2xs font-mono text-text-faint truncate" title={workspaceState.branch ?? ''}
        >{workspaceState.branch ?? ''}</span
      >
      {#if workspaceState.isDirty}
        <span
          class="w-1.5 h-1.5 rounded-full bg-warning flex-shrink-0"
          role="img"
          aria-label="Uncommitted changes"
          title="Uncommitted changes"
        ></span>
      {/if}
    </span>
  {/snippet}
  <div class="flex flex-col">
    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
      disabled={!workspaceState.isDirty || loading === 'commit'}
      onclick={doCommit}
      title={workspaceState.isDirty ? 'Commit staged changes' : 'Nothing to commit'}
    >
      {#if loading === 'commit'}
        <LoaderCircle
          size={13}
          class="text-text-faint animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
        />
      {:else}
        <GitCommitVertical
          size={13}
          class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Commit</span>
    </button>

    <div
      class="h-px mx-3 my-1 bg-border-subtle"
      role="separator"
      aria-orientation="horizontal"
    ></div>

    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
      disabled={loading === 'push'}
      onclick={doPush}
      title="Push to remote"
    >
      {#if loading === 'push'}
        <LoaderCircle
          size={13}
          class="text-text-faint animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
        />
      {:else}
        <ArrowUpFromLine
          size={13}
          class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Push</span>
      {#if ahead > 0}
        <span
          class="inline-flex items-center justify-center min-w-4 h-4 px-1 rounded-sm bg-accent-bg text-accent-text text-2xs font-semibold tracking-caps-tight leading-tight flex-shrink-0"
          >{ahead}</span
        >
      {/if}
    </button>
    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
      disabled={loading === 'pull'}
      onclick={doPull}
      title="Pull from remote"
    >
      {#if loading === 'pull'}
        <LoaderCircle
          size={13}
          class="text-text-faint animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
        />
      {:else}
        <ArrowDownToLine
          size={13}
          class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Pull</span>
      {#if behind > 0}
        <span
          class="inline-flex items-center justify-center min-w-4 h-4 px-1 rounded-sm bg-accent-bg text-accent-text text-2xs font-semibold tracking-caps-tight leading-tight flex-shrink-0"
          >{behind}</span
        >
      {/if}
    </button>
    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
      disabled={loading === 'fetch'}
      onclick={doFetch}
      title="Fetch from remote"
    >
      {#if loading === 'fetch'}
        <LoaderCircle
          size={13}
          class="text-text-faint animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
        />
      {:else}
        <RefreshCw
          size={13}
          class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Fetch</span>
    </button>

    <div
      class="h-px mx-3 my-1 bg-border-subtle"
      role="separator"
      aria-orientation="horizontal"
    ></div>

    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
      disabled={!workspaceState.isDirty || loading === 'stash'}
      onclick={doStash}
      title={workspaceState.isDirty ? 'Stash changes' : 'Nothing to stash'}
    >
      {#if loading === 'stash'}
        <LoaderCircle
          size={13}
          class="text-text-faint animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
        />
      {:else}
        <Archive
          size={13}
          class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Stash</span>
    </button>
    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
      disabled={loading === 'stashPop'}
      onclick={doStashPop}
      title="Pop stashed changes"
    >
      {#if loading === 'stashPop'}
        <LoaderCircle
          size={13}
          class="text-text-faint animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
        />
      {:else}
        <ArchiveRestore
          size={13}
          class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Stash Pop</span>
    </button>

    <div
      class="h-px mx-3 my-1 bg-border-subtle"
      role="separator"
      aria-orientation="horizontal"
    ></div>

    {#if existingPR}
      {@const chip = prStateChip(existingPR.state, existingPR.isDraft)}
      <button
        class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
        onclick={openExistingPR}
        title={branchPR
          ? `View PR #${branchPR.number} — ${branchPR.title}`
          : 'View the latest pull request for this branch'}
      >
        <GitPullRequest
          size={13}
          class="text-text-faint group-enabled:group-hover:text-accent-text flex-shrink-0"
        />
        <span class="flex-1">View PR #{existingPR.number}</span>
        {#if chip.label}
          <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 {chip.cls}">{chip.label}</span
          >
        {/if}
      </button>
    {/if}
    {#if showCreatePRRow}
      <button
        class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
        disabled={!workspaceState.branch}
        onclick={doCreatePR}
        title="Create a pull request from this branch — edit the title and description before it is created"
      >
        <GitPullRequest
          size={13}
          class="text-text-faint group-enabled:group-hover:text-accent-text flex-shrink-0"
        />
        <span class="flex-1">Create PR</span>
      </button>
    {/if}
  </div>
</CollapsibleSection>
