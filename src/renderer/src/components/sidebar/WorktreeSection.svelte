<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { workspaceState, selectWorktree } from '../../lib/stores/workspace.svelte'
  import { closeAllTabsForWorktree } from '../../lib/stores/tabs.svelte'
  import { showCreateWorktree, confirm } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { Trash2 } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import {
    formatPrBadge,
    getBranchPRMap,
    loadBranchPRs,
    prKey,
  } from '../../lib/stores/github.svelte'
  import { getResolvedConfig } from '../../lib/stores/taskTracker.svelte'

  let mergedBranches = new SvelteSet<string>()

  function worktreeLabel(wt: { branch: string; path: string }): string {
    if (wt.branch !== '(detached)') return wt.branch
    return wt.path.split('/').pop() || wt.path
  }

  async function checkMergedStatus(signal: AbortSignal): Promise<void> {
    const repoRoot = workspaceState.repoRoot
    if (!repoRoot) return

    const branches = workspaceState.worktrees
      .filter((wt) => !wt.isMain && wt.branch !== '(detached)')
      .map((wt) => wt.branch)
    const result =
      branches.length > 0
        ? await window.api.worktreeGetMergedBranches({ repoRoot, branches })
        : { mergedBranches: [] }
    if (signal.aborted) return
    const next = new SvelteSet<string>()
    for (const branch of result.mergedBranches) {
      next.add(branch)
    }
    mergedBranches = next
  }

  $effect(() => {
    // Track length so the effect re-runs when worktrees are added/removed,
    // not on every nested mutation (PR badge updates etc.). The previous
    // `length >= 0` guard was always true and triggered a full IPC
    // refresh on every reactive read of the array.
    void workspaceState.worktrees.length
    const ac = new AbortController()
    checkMergedStatus(ac.signal)
    return () => ac.abort()
  })

  let githubConnectionCount = $derived(
    (getResolvedConfig()?.config.trackers ?? []).filter((t) => t.provider === 'github').length,
  )

  let prevGhConnCount = 0
  $effect(() => {
    const repoRoot = workspaceState.repoRoot
    void workspaceState.worktrees.length
    const connCount = githubConnectionCount
    if (!repoRoot || connCount === 0) return
    const force = connCount !== prevGhConnCount
    prevGhConnCount = connCount
    loadBranchPRs(repoRoot, force)
  })

  let prMap = $derived(getBranchPRMap())

  async function removeWorktree(
    e: MouseEvent,
    wt: { path: string; branch: string },
  ): Promise<void> {
    e.stopPropagation()
    const repoRoot = workspaceState.worktrees.find((w) => w.isMain)?.path ?? workspaceState.repoRoot
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

    if (!(await closeAllTabsForWorktree(wt.path))) return

    // Leave the doomed worktree BEFORE removing it — keeping it selected leaves
    // watchers and pollers pointed at a path that is being deleted.
    if (workspaceState.selectedWorktreePath === wt.path) {
      const main = workspaceState.worktrees.find((w) => w.isMain)
      if (main) selectWorktree(main.path)
    }

    try {
      const result = await window.api.worktreeRemoveWithBranch({
        repoRoot,
        worktreePath: wt.path,
        branch: wt.branch,
        deleteBranch: !isDetached,
        forceOnFailure: true,
      })
      if (result.leftoverPath) {
        addToast(
          `Worktree removed, but some files are still in use and were left at ${result.leftoverPath}`,
        )
      }
    } catch (err) {
      await confirm({
        title: 'Git Error',
        message: err instanceof Error ? err.message : String(err),
        confirmLabel: 'OK',
      })
    }
  }
</script>

<CollapsibleSection title="WORKTREES" sectionKey="worktrees">
  {#snippet headerExtra()}
    <button
      class="inline-flex items-center h-5 px-1.5 rounded-sm font-inherit text-2xs font-medium text-text-faint bg-transparent border-0 cursor-pointer transition-colors duration-fast hover:text-text hover:bg-hover"
      onclick={showCreateWorktree}
      title="Create worktree">+ new</button
    >
  {/snippet}
  <ul class="list-none p-0 m-0">
    {#each workspaceState.worktrees as wt (wt.path)}
      <li
        class="flex items-center hover:bg-hover"
        class:bg-active={wt.path === workspaceState.selectedWorktreePath}
      >
        <button
          class="flex items-center gap-2 flex-1 min-w-0 h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left"
          onclick={() => selectWorktree(wt.path)}
        >
          <span class="font-mono text-xs text-text-secondary w-2.5 flex-shrink-0"
            >{wt.isMain ? '*' : ' '}</span
          >
          <span class="overflow-hidden text-ellipsis whitespace-nowrap" title={wt.path}
            >{worktreeLabel(wt)}</span
          >
          {#if wt.branch === '(detached)'}
            <span
              class="ml-auto inline-flex items-center h-4 px-1.5 rounded-md text-2xs font-semibold font-mono tracking-caps-tight bg-border-subtle text-warning-text leading-tight flex-shrink-0"
              title={wt.head}>{wt.head.slice(0, 7)}</span
            >
          {:else if mergedBranches.has(wt.branch)}
            <span
              class="ml-auto inline-flex items-center h-4 px-1.5 rounded-md text-2xs font-semibold uppercase tracking-caps-tight bg-border-subtle text-success-text leading-tight flex-shrink-0"
              title="Merged">merged</span
            >
          {/if}
        </button>
        {#if prMap[prKey(workspaceState.repoRoot, wt.branch)]}
          {@const pr = prMap[prKey(workspaceState.repoRoot, wt.branch)]}
          {@const badge = formatPrBadge(pr)}
          <button
            class="{badge.className} flex-shrink-0 mr-1"
            title={`${pr.title} — click to open`}
            aria-label={`Open pull request: ${pr.title}`}
            onclick={(e) => {
              e.stopPropagation()
              window.api.openExternal(pr.url)
            }}
          >
            {badge.label}
          </button>
        {/if}
        {#if !wt.isMain && mergedBranches.has(wt.branch)}
          <button
            class="inline-flex items-center justify-center size-6 p-0 border-0 bg-transparent text-text-faint cursor-pointer flex-shrink-0 rounded-md mr-1 transition-colors duration-fast hover:text-danger-text hover:bg-danger-bg"
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
