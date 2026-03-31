<script lang="ts">
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { confirm, prompt } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
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
        await confirm({
          title: 'Push',
          message: 'No upstream tracking branch configured.',
          confirmLabel: 'OK',
        })
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

  function extractIssueKeyFromBranch(branch: string | null): string | null {
    if (!branch) return null
    const match = branch.match(/([A-Z][A-Z0-9]+-\d+)/)
    return match ? match[1] : null
  }

  let issueKeyFromBranch = $derived(extractIssueKeyFromBranch(workspaceState.branch))

  async function doCreatePR(): Promise<void> {
    const branch = workspaceState.branch
    if (!branch) return

    const issueKey = issueKeyFromBranch
    const issue = {
      key: issueKey ?? '',
      summary: '',
      description: '',
      status: '',
      priority: '',
      type: 'task',
    }

    const defaultTarget = (await window.api.getPref('issueTracker.prDefaultBranch')) || 'develop'

    const ok = await confirm({
      title: 'Create Pull Request',
      message: `Create PR from "${branch}"?`,
      details: `Issue: ${issueKey ?? 'none'}\nTarget: ${defaultTarget}\n\nTitle will be resolved from issue tracker.`,
      confirmLabel: 'Create PR',
    })
    if (!ok) return

    loading = 'pr'
    try {
      const result = await window.api.issueTrackerCreatePR(worktreePath(), issue, branch)
      addToast(`PR created`)
      window.api.openExternal(result.url)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      if (msg.includes('No commits between')) {
        await confirm({
          title: 'No Changes',
          message: `No commits between target branch and "${branch}". Commit changes first.`,
          confirmLabel: 'OK',
        })
      } else {
        await gitError(err)
      }
    } finally {
      loading = null
    }
  }
</script>

<span class="sr-only" aria-live="polite">{loading ? `${loading} in progress…` : ''}</span>
<CollapsibleSection title="GIT" sectionKey="git" borderTop>
  {#snippet headerExtra()}
    <span class="branch-status">
      <span class="branch-label">{workspaceState.branch ?? ''}</span>
      {#if workspaceState.isDirty}
        <span class="dirty-dot" title="Uncommitted changes"></span>
      {/if}
    </span>
  {/snippet}
  <div class="action-list">
    <button
      class="action-item"
      disabled={!workspaceState.isDirty || loading === 'commit'}
      onclick={doCommit}
      title={workspaceState.isDirty ? 'Commit staged changes' : 'Nothing to commit'}
    >
      <span class="action-label">Commit</span>
    </button>
    <button
      class="action-item"
      disabled={loading === 'push'}
      onclick={doPush}
      title="Push to remote"
    >
      <span class="action-label">Push</span>
      {#if ahead > 0}
        <span class="badge">{ahead}</span>
      {/if}
    </button>
    <button
      class="action-item"
      disabled={loading === 'pull'}
      onclick={doPull}
      title="Pull from remote"
    >
      <span class="action-label">Pull</span>
      {#if behind > 0}
        <span class="badge">{behind}</span>
      {/if}
    </button>
    <button
      class="action-item"
      disabled={loading === 'fetch'}
      onclick={doFetch}
      title="Fetch from remote"
    >
      <span class="action-label">Fetch</span>
    </button>
    <button
      class="action-item"
      disabled={!workspaceState.isDirty || loading === 'stash'}
      onclick={doStash}
      title={workspaceState.isDirty ? 'Stash changes' : 'Nothing to stash'}
    >
      <span class="action-label">Stash</span>
    </button>
    <button
      class="action-item"
      disabled={loading === 'stashPop'}
      onclick={doStashPop}
      title="Pop stashed changes"
    >
      <span class="action-label">Stash Pop</span>
    </button>
    <button
      class="action-item"
      disabled={loading === 'pr' || !workspaceState.branch}
      onclick={doCreatePR}
      title={issueKeyFromBranch ? `Create PR for ${issueKeyFromBranch}` : 'Create Pull Request'}
    >
      <span class="action-label">Create PR</span>
      {#if issueKeyFromBranch}
        <span class="badge">{issueKeyFromBranch}</span>
      {/if}
    </button>
  </div>
</CollapsibleSection>

<style>
  .branch-status {
    display: flex;
    align-items: center;
    gap: 5px;
    overflow: hidden;
  }

  .branch-label {
    font-size: 10px;
    color: var(--c-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 120px;
  }

  .dirty-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c-warning);
    flex-shrink: 0;
  }

  .action-list {
    display: flex;
    flex-direction: column;
  }

  .action-item {
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

  .action-item:hover:not(:disabled) {
    background: var(--c-hover);
  }

  .action-item:disabled {
    color: var(--c-text-faint);
    cursor: default;
  }

  .action-label {
    flex: 1;
  }

  .badge {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--c-border);
    color: var(--c-text);
    font-size: 10px;
    font-weight: 600;
    flex-shrink: 0;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
