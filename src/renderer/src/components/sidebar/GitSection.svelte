<script lang="ts">
  import {
    GitCommitVertical,
    ArrowUpFromLine,
    ArrowDownToLine,
    RefreshCw,
    Archive,
    ArchiveRestore,
    GitPullRequest,
    Loader2,
  } from '@lucide/svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { confirm, prompt, showCreateGitHubPR } from '../../lib/stores/dialogs.svelte'
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
      await window.api.gitCommit(worktreePath(), result.value, result.checked)
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doPush(): Promise<void> {
    loading = 'push'
    try {
      const info = await window.api.gitPushInfo(worktreePath())
      if (!info) {
        const ok = await confirm({
          title: 'Push',
          message: 'No upstream branch — push and set tracking to origin?',
        })
        if (ok) {
          await window.api.gitPush(worktreePath())
        }
        return
      }
      const ok = await confirm({
        title: 'Push',
        message: `Push ${info.commitCount} commit(s) to ${info.remote}/${info.branch}?`,
      })
      if (ok) {
        await window.api.gitPush(worktreePath())
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
      const rebase = (await window.api.getPref('gitPullRebase')) !== 'false'
      await window.api.gitPull(worktreePath(), rebase)
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doFetch(): Promise<void> {
    loading = 'fetch'
    try {
      await window.api.gitFetch(worktreePath())
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doStash(): Promise<void> {
    loading = 'stash'
    try {
      await window.api.gitStash(worktreePath())
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  async function doStashPop(): Promise<void> {
    loading = 'stashPop'
    try {
      await window.api.gitStashPop(worktreePath())
    } catch (err) {
      await gitError(err)
    } finally {
      loading = null
    }
  }

  let ahead = $derived(workspaceState.aheadBehind?.ahead ?? 0)
  let behind = $derived(workspaceState.aheadBehind?.behind ?? 0)

  function doCreatePR(): void {
    if (!workspaceState.branch) return
    showCreateGitHubPR()
  }
</script>

<span class="sr-only" aria-live="polite">{loading ? `${loading} in progress…` : ''}</span>
<CollapsibleSection title="GIT" sectionKey="git" borderTop>
  {#snippet headerExtra()}
    <span class="flex items-center gap-1 overflow-hidden">
      <span class="text-2xs font-mono text-text-faint truncate max-w-30"
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
        <Loader2 size={13} class="text-text-faint animate-spin-slow flex-shrink-0" />
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
        <Loader2 size={13} class="text-text-faint animate-spin-slow flex-shrink-0" />
      {:else}
        <ArrowUpFromLine
          size={13}
          class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Push</span>
      {#if ahead > 0}
        <span
          class="inline-flex items-center justify-center min-w-4 h-4 px-1.5 rounded-md bg-accent-bg text-accent-text text-2xs font-semibold tracking-caps-tight leading-tight flex-shrink-0"
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
        <Loader2 size={13} class="text-text-faint animate-spin-slow flex-shrink-0" />
      {:else}
        <ArrowDownToLine
          size={13}
          class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Pull</span>
      {#if behind > 0}
        <span
          class="inline-flex items-center justify-center min-w-4 h-4 px-1.5 rounded-md bg-accent-bg text-accent-text text-2xs font-semibold tracking-caps-tight leading-tight flex-shrink-0"
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
        <Loader2 size={13} class="text-text-faint animate-spin-slow flex-shrink-0" />
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
        <Loader2 size={13} class="text-text-faint animate-spin-slow flex-shrink-0" />
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
        <Loader2 size={13} class="text-text-faint animate-spin-slow flex-shrink-0" />
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

    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
      disabled={!workspaceState.branch || loading === 'pr'}
      onclick={doCreatePR}
      title="Create pull request"
    >
      {#if loading === 'pr'}
        <Loader2 size={13} class="text-text-faint animate-spin-slow flex-shrink-0" />
      {:else}
        <GitPullRequest
          size={13}
          class="text-text-faint group-enabled:group-hover:text-accent-text flex-shrink-0"
        />
      {/if}
      <span class="flex-1">Create PR</span>
    </button>
  </div>
</CollapsibleSection>
