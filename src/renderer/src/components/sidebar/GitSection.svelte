<script lang="ts">
  import { onMount } from 'svelte'
  import {
    GitCommitVertical,
    ArrowUpFromLine,
    ArrowDownToLine,
    RefreshCw,
    Archive,
    ArchiveRestore,
    GitPullRequest,
    LoaderCircle,
    Hammer,
    Play,
    KeyRound,
  } from '@lucide/svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import {
    confirm,
    prompt,
    showPRDetails,
    showCreateTaskPR,
    showPreferences,
  } from '../../lib/stores/dialogs.svelte'
  import { getPRForBranch, getPRRefreshTick } from '../../lib/stores/github.svelte'
  import { getPanelTask, getPanelTaskResolvedPath } from '../../lib/stores/taskTracker.svelte'
  import { getCiState, refreshCi, triggerCiBuild } from '../../lib/stores/ci.svelte'
  import { ciChip, anyBuildActive } from '../../lib/ci/status'
  import { prStateChip } from '../../lib/github/prState'
  import CollapsibleSection from './CollapsibleSection.svelte'

  let loading: string | null = $state(null)

  // Number of files with uncommitted changes — shown on the Commit row and refreshed the same
  // way the changes panel refreshes (git metadata events + debounced filesystem events).
  let changeCount = $state(0)

  async function refreshChangeCount(): Promise<void> {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    if (!path) {
      changeCount = 0
      return
    }
    try {
      const status = await window.api.fileTreeGetGitStatus(path, path)
      changeCount = Object.keys(status.statuses).length
    } catch {
      changeCount = 0
    }
  }

  $effect(() => {
    void workspaceState.selectedWorktreePath
    void workspaceState.isDirty
    void refreshChangeCount()
  })

  onMount(() => {
    const unsubGit = window.api.onGitChanged(() => void refreshChangeCount())
    let timer: ReturnType<typeof setTimeout> | null = null
    const unsubFiles = window.api.onFilesChanged((payload) => {
      const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
      if (payload.repoRoot !== path) return
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => {
        timer = null
        void refreshChangeCount()
      }, 200)
    })
    return () => {
      unsubGit()
      unsubFiles()
      if (timer != null) clearTimeout(timer)
    }
  })

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
  // Keyed by the MAIN repo root — that is what loadBranchPRs fetches under.
  let branchPR = $derived(
    workspaceState.branch
      ? getPRForBranch(workspaceState.repoRoot, workspaceState.branch)
      : undefined,
  )
  // The github store map needs the GitHub API integration; fall back to the gh CLI (same auth as
  // PR creation) so the "View PR" row appears even without it — with the PR state for a chip.
  let fallbackPR = $state<{ number: number; state: string; isDraft: boolean } | null>(null)
  // The gh-CLI fallback takes a network round-trip: without a pending state the PR
  // rows simply popped in seconds after the section rendered, looking broken.
  let prLoading = $state(false)
  $effect(() => {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    const branch = workspaceState.branch
    // Re-check after any PR mutation elsewhere in the app (create/merge/close bump the tick).
    void getPRRefreshTick()
    fallbackPR = null
    if (!path || !branch || branchPR) {
      prLoading = false
      return
    }
    let cancelled = false
    prLoading = true
    window.api
      .taskTrackerPRDetails(path, branch)
      .then((pr) => {
        if (!cancelled && pr) {
          fallbackPR = { number: pr.number, state: pr.state, isDraft: pr.isDraft }
        }
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) prLoading = false
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
  // A branch can accumulate merged/closed PRs — only an ACTIVE (open) one blocks a new PR.
  let showCreatePRRow = $derived(!existingPR || existingPR.state !== 'OPEN')

  function openExistingPR(): void {
    if (!workspaceState.branch) return
    showPRDetails(worktreePath(), workspaceState.branch)
  }

  // --- CI builds for the current branch (only when the repo configures `ci` in
  // .canopy/config.json — the section stays invisible everywhere else).
  let ci = $derived(getCiState())
  let ciResponse = $derived(ci.response)
  // A queued/running build flips polling to the fast interval.
  let ciActive = $derived(ciResponse ? anyBuildActive(ciResponse.rows) : false)
  let triggering = $state<string | null>(null)

  $effect(() => {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    const branch = workspaceState.branch
    if (!path || !branch) return
    void refreshCi(path, branch)
    // Unconfigured repos answer from a local config read (no network), so the slow
    // poll is cheap and picks up a hand-added `ci` block without a restart.
    const interval = ciActive ? 10_000 : 45_000
    const timer = setInterval(() => void refreshCi(path, branch), interval)
    return () => clearInterval(timer)
  })

  async function doTriggerBuild(buildTypeId: string, label: string): Promise<void> {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    const branch = workspaceState.branch
    if (!path || !branch) return
    triggering = buildTypeId
    try {
      await triggerCiBuild(path, buildTypeId, branch, label)
    } finally {
      triggering = null
    }
  }

  function openBuild(webUrl: string): void {
    if (webUrl) window.api.openExternal(webUrl)
  }

  function doCreatePR(): void {
    if (!workspaceState.branch) return
    // The linked tracker task (when there is one) provides the PR template context; without it
    // the form falls back to a plain branch-level PR. During a worktree switch the panel task can
    // still belong to the previous worktree — only trust it once it was resolved for this path.
    const resolvedFor = getPanelTaskResolvedPath()
    const t = resolvedFor === worktreePath().replace(/\\/g, '/') ? getPanelTask() : null
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
<span class="sr-only" aria-live="polite">{prLoading ? 'Checking pull requests…' : ''}</span>
<span class="sr-only" aria-live="polite"
  >{ciResponse?.configured && ci.loading && ciResponse.rows.length === 0
    ? 'Checking builds…'
    : ''}</span
>
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
      {#if changeCount > 0}
        <span
          class="flex-shrink-0 min-w-4 px-1 py-px rounded-md bg-active text-2xs text-text-muted text-center leading-tight"
          title={`${changeCount} file${changeCount === 1 ? '' : 's'} with uncommitted changes`}
          >{changeCount}</span
        >
      {/if}
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

    {#if prLoading && !existingPR}
      <!-- The PR rows below depend on a network round-trip — show where they will
           appear instead of popping them in with no warning. -->
      <div class="flex items-center gap-2.5 w-full h-7 px-3 text-sm text-text-faint">
        <LoaderCircle
          size={13}
          class="animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
        />
        <span class="flex-1">Checking pull requests…</span>
      </div>
    {/if}
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
    {#if showCreatePRRow && !prLoading}
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

    {#if ciResponse?.configured}
      <div
        class="h-px mx-3 my-1 bg-border-subtle"
        role="separator"
        aria-orientation="horizontal"
      ></div>

      {#if ciResponse.hasToken === false}
        <button
          class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
          onclick={() => showPreferences('Your connections')}
          title="This repository configures a TeamCity server, but no token is stored — connect it in Settings"
        >
          <KeyRound size={13} class="text-warning flex-shrink-0" />
          <span class="flex-1">Connect TeamCity</span>
        </button>
      {:else if ciResponse.error}
        <div
          class="flex items-center gap-2.5 w-full min-h-7 px-3 py-1 text-sm text-text-faint"
          title={ciResponse.error}
        >
          <Hammer size={13} class="flex-shrink-0" />
          <span class="flex-1 truncate">{ciResponse.error}</span>
        </div>
      {:else if ci.loading && ciResponse.rows.length === 0}
        <div class="flex items-center gap-2.5 w-full h-7 px-3 text-sm text-text-faint">
          <LoaderCircle
            size={13}
            class="animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
          />
          <span class="flex-1">Checking builds…</span>
        </div>
      {:else}
        {#each ciResponse.rows as row (row.buildTypeId)}
          {@const chip = ciChip(row.build)}
          {@const buildActive = row.build != null && row.build.state !== 'finished'}
          <div class="group flex items-center w-full h-7 pr-1">
            <button
              class="flex-1 flex items-center gap-2.5 min-w-0 h-full pl-3 pr-1 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
              disabled={!row.build}
              onclick={() => row.build && openBuild(row.build.webUrl)}
              title={row.build
                ? `Open build #${row.build.number} in TeamCity`
                : `No builds of ${row.label} for this branch yet`}
            >
              <Hammer
                size={13}
                class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
              />
              <span class="flex-1 truncate">{row.label}</span>
              <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 {chip.cls}"
                >{chip.label}</span
              >
            </button>
            <button
              class="flex items-center justify-center size-6 rounded-md border-0 bg-transparent text-text-faint cursor-pointer transition-colors duration-fast enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-40 disabled:cursor-default flex-shrink-0"
              disabled={buildActive || triggering === row.buildTypeId}
              onclick={() => doTriggerBuild(row.buildTypeId, row.label)}
              aria-label={`Run ${row.label}`}
              title={buildActive
                ? 'A build is already queued or running for this branch'
                : `Queue a ${row.label} build for this branch`}
            >
              {#if triggering === row.buildTypeId}
                <LoaderCircle size={12} class="animate-spin-slow motion-reduce:animate-none" />
              {:else}
                <Play size={12} />
              {/if}
            </button>
          </div>
        {/each}
      {/if}
    {/if}
  </div>
</CollapsibleSection>
