<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { workspaceState, selectWorktree } from '../../lib/stores/workspace.svelte'
  import { showCreateWorktree, confirm } from '../../lib/stores/dialogs.svelte'
  import { Trash2 } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'

  let mergedBranches = new SvelteSet<string>()

  function worktreeLabel(wt: { branch: string; path: string }): string {
    if (wt.branch !== '(detached)') return wt.branch
    return wt.path.split('/').pop() || wt.path
  }

  async function checkMergedStatus(signal: AbortSignal): Promise<void> {
    const repoRoot = workspaceState.repoRoot
    if (!repoRoot) return

    const checks = workspaceState.worktrees
      .filter((wt) => !wt.isMain)
      .map(async (wt) => {
        const merged = await window.api.gitBranchMerged(repoRoot, wt.branch)
        return { branch: wt.branch, merged }
      })

    const results = await Promise.all(checks)
    if (signal.aborted) return
    const next = new SvelteSet<string>()
    for (const r of results) {
      if (r.merged) next.add(r.branch)
    }
    mergedBranches = next
  }

  $effect(() => {
    // Re-check when worktree list changes (read .length synchronously for dependency tracking)
    const ac = new AbortController()
    if (workspaceState.worktrees.length >= 0) checkMergedStatus(ac.signal)
    return () => ac.abort()
  })

  async function removeWorktree(
    e: MouseEvent,
    wt: { path: string; branch: string },
  ): Promise<void> {
    e.stopPropagation()
    const repoRoot = workspaceState.repoRoot
    if (!repoRoot) return

    const isDetached = wt.branch === '(detached)'
    const ok = await confirm({
      title: 'Remove Worktree',
      message: isDetached
        ? `Remove worktree "${wt.path.split('/').pop()}"?`
        : `Remove worktree and delete branch "${wt.branch}"?`,
      details: wt.path,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return

    try {
      await window.api.gitWorktreeRemove(repoRoot, wt.path, false)
      if (!isDetached) await window.api.gitBranchDelete(repoRoot, wt.branch, false)
    } catch {
      // Force remove if normal remove fails (e.g. dirty worktree)
      try {
        await window.api.gitWorktreeRemove(repoRoot, wt.path, true)
        if (!isDetached) await window.api.gitBranchDelete(repoRoot, wt.branch, true)
      } catch {
        // Ignore — watcher will update the list
      }
    }

    // If the removed worktree was selected, fall back to main
    if (workspaceState.selectedWorktreePath === wt.path) {
      const main = workspaceState.worktrees.find((w) => w.isMain)
      if (main) selectWorktree(main.path)
    }
  }
</script>

<CollapsibleSection title="WORKTREES" sectionKey="worktrees">
  {#snippet headerExtra()}
    <button class="new-btn" onclick={showCreateWorktree} title="Create worktree">+ new</button>
  {/snippet}
  <ul class="worktree-list">
    {#each workspaceState.worktrees as wt (wt.path)}
      <li class="worktree-row">
        <button
          class="worktree-item"
          class:active={wt.path === workspaceState.selectedWorktreePath}
          onclick={() => selectWorktree(wt.path)}
        >
          <span class="indicator">{wt.isMain ? '*' : ' '}</span>
          <span class="branch-name" title={wt.path}>{worktreeLabel(wt)}</span>
          {#if wt.branch === '(detached)'}
            <span class="detached-badge" title={wt.head}>{wt.head.slice(0, 7)}</span>
          {:else if mergedBranches.has(wt.branch)}
            <span class="merged-badge" title="Merged">merged</span>
          {/if}
        </button>
        {#if !wt.isMain && mergedBranches.has(wt.branch)}
          <button
            class="remove-btn"
            title="Remove worktree and delete branch"
            aria-label="Remove worktree and delete branch"
            onclick={(e) => removeWorktree(e, wt)}
          >
            <Trash2 size={12} />
          </button>
        {/if}
      </li>
    {/each}
  </ul>
</CollapsibleSection>

<style>
  .new-btn {
    font-size: 10px;
    font-weight: 500;
    font-family: inherit;
    color: rgba(255, 255, 255, 0.35);
    background: none;
    border: none;
    padding: 1px 5px;
    border-radius: 4px;
    cursor: pointer;
    transition:
      color 0.1s,
      background 0.1s;
  }

  .new-btn:hover {
    color: rgba(255, 255, 255, 0.7);
    background: rgba(255, 255, 255, 0.08);
  }

  .worktree-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .worktree-row {
    display: flex;
    align-items: center;
  }

  .worktree-item {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    min-width: 0;
    padding: 4px 12px;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }

  .worktree-item:hover {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.9);
  }

  .worktree-item.active {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .indicator {
    font-family: monospace;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    width: 10px;
    flex-shrink: 0;
  }

  .branch-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detached-badge {
    font-size: 9px;
    font-weight: 500;
    font-family: monospace;
    color: rgba(255, 180, 80, 0.7);
    flex-shrink: 0;
    margin-left: auto;
  }

  .merged-badge {
    font-size: 9px;
    font-weight: 500;
    color: rgba(100, 200, 120, 0.7);
    flex-shrink: 0;
    margin-left: auto;
  }

  .remove-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.25);
    cursor: pointer;
    flex-shrink: 0;
    border-radius: 4px;
    margin-right: 4px;
    transition:
      color 0.1s,
      background 0.1s;
  }

  .remove-btn:hover {
    color: #e05050;
    background: rgba(224, 80, 80, 0.12);
  }
</style>
