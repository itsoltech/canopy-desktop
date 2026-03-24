<script lang="ts">
  import { ChevronRight, Trash2, X } from '@lucide/svelte'
  import {
    projects,
    workspaceState,
    selectWorktree,
    attachProject,
    detachProject,
    initGitRepo,
    type ProjectState,
  } from '../../lib/stores/workspace.svelte'
  import { showCreateWorktree, confirm } from '../../lib/stores/dialogs.svelte'

  // Per-project collapse state
  function storageKey(project: ProjectState): string {
    return `canopy:sidebar:collapsed:projects:${project.workspace.path}`
  }

  function isCollapsed(project: ProjectState): boolean {
    return localStorage.getItem(storageKey(project)) === '1'
  }

  function toggleCollapse(project: ProjectState): void {
    const key = storageKey(project)
    const current = localStorage.getItem(key) === '1'
    localStorage.setItem(key, current ? '0' : '1')
    collapseState = { ...collapseState, [project.workspace.path]: !current }
  }

  // Reactive collapse tracking
  let collapseState: Record<string, boolean> = $state(
    Object.fromEntries(projects.map((p) => [p.workspace.path, isCollapsed(p)])),
  )

  // Sync when projects change
  $effect(() => {
    for (const p of projects) {
      if (!(p.workspace.path in collapseState)) {
        collapseState[p.workspace.path] = isCollapsed(p)
      }
    }
  })

  // Per-project merged branches
  let mergedBranches: Record<string, Set<string>> = $state({})

  async function checkMergedStatus(project: ProjectState): Promise<void> {
    if (!project.isGitRepo || !project.repoRoot) return
    const checks = project.worktrees
      .filter((wt) => !wt.isMain)
      .map(async (wt) => {
        const merged = await window.api.gitBranchMerged(project.repoRoot!, wt.branch)
        return { branch: wt.branch, merged }
      })
    const results = await Promise.all(checks)
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- temporary local Set, not reactive state
    const next = new Set<string>()
    for (const r of results) {
      if (r.merged) next.add(r.branch)
    }
    mergedBranches = { ...mergedBranches, [project.workspace.path]: next }
  }

  // Re-check merged status when any project's worktrees change
  $effect(() => {
    const deps = projects.map((p) => p.worktrees.length)
    if (deps.length >= 0) {
      for (const p of projects) {
        checkMergedStatus(p)
      }
    }
  })

  function getMerged(project: ProjectState): Set<string> {
    return mergedBranches[project.workspace.path] ?? new Set()
  }

  async function removeWorktree(
    e: MouseEvent,
    project: ProjectState,
    wt: { path: string; branch: string },
  ): Promise<void> {
    e.stopPropagation()
    if (!project.repoRoot) return

    const ok = await confirm({
      title: 'Remove Worktree',
      message: `Remove worktree and delete branch "${wt.branch}"?`,
      details: wt.path,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return

    try {
      await window.api.gitWorktreeRemove(project.repoRoot, wt.path, false)
      await window.api.gitBranchDelete(project.repoRoot, wt.branch, false)
    } catch {
      try {
        await window.api.gitWorktreeRemove(project.repoRoot, wt.path, true)
        await window.api.gitBranchDelete(project.repoRoot, wt.branch, true)
      } catch {
        // Ignore — watcher will update the list
      }
    }

    if (workspaceState.selectedWorktreePath === wt.path) {
      const main = project.worktrees.find((w) => w.isMain)
      if (main) selectWorktree(main.path)
    }
  }

  function handleNewWorktree(e: MouseEvent, project: ProjectState): void {
    e.stopPropagation()
    showCreateWorktree({ repoRoot: project.repoRoot!, workspaceId: project.workspace.id })
  }

  function handleInitGit(e: MouseEvent, project: ProjectState): void {
    e.stopPropagation()
    initGitRepo(project.workspace.path)
  }

  async function handleDetach(e: MouseEvent, project: ProjectState): Promise<void> {
    e.stopPropagation()
    await detachProject(project.repoRoot ?? project.workspace.path)
  }

  async function handleAttachProject(): Promise<void> {
    const path = await window.api.openFolder()
    if (path) await attachProject(path)
  }
</script>

<section class="sidebar-section">
  <div class="section-header">
    <h3 class="section-title">PROJECTS</h3>
  </div>

  {#each projects as project (project.workspace.path)}
    {@const collapsed = collapseState[project.workspace.path] ?? false}
    {@const merged = getMerged(project)}
    <div class="project-group">
      <div class="project-header">
        <button
          class="project-toggle"
          onclick={() => toggleCollapse(project)}
          aria-expanded={!collapsed}
        >
          <span class="chevron" class:open={!collapsed && project.isGitRepo}>
            {#if project.isGitRepo}
              <ChevronRight size={12} />
            {/if}
          </span>
          <span class="project-name">{project.workspace.name}</span>
        </button>
        <div class="project-actions">
          {#if project.isGitRepo}
            <button
              class="action-btn"
              onclick={(e) => handleNewWorktree(e, project)}
              title="Create worktree">+ new</button
            >
          {:else}
            <button
              class="action-btn"
              onclick={(e) => handleInitGit(e, project)}
              title="Initialize git repository">init</button
            >
          {/if}
          <button
            class="detach-btn"
            onclick={(e) => handleDetach(e, project)}
            title="Detach project from window"
          >
            <X size={10} />
          </button>
        </div>
      </div>

      {#if !collapsed && project.isGitRepo}
        <ul class="worktree-list">
          {#each project.worktrees as wt (wt.path)}
            <li class="worktree-row">
              <button
                class="worktree-item"
                class:active={wt.path === workspaceState.selectedWorktreePath}
                onclick={() => selectWorktree(wt.path)}
              >
                <span class="indicator">{wt.isMain ? '*' : ' '}</span>
                <span class="branch-name">{wt.branch}</span>
                {#if merged.has(wt.branch)}
                  <span class="merged-badge" title="Merged">merged</span>
                {/if}
              </button>
              {#if !wt.isMain && merged.has(wt.branch)}
                <button
                  class="remove-btn"
                  title="Remove worktree and delete branch"
                  onclick={(e) => removeWorktree(e, project, wt)}
                >
                  <Trash2 size={12} />
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/each}

  <button class="attach-btn" onclick={handleAttachProject}>+ attach</button>
</section>

<style>
  .sidebar-section {
    padding: 12px 0;
  }

  .section-header {
    display: flex;
    align-items: center;
    padding: 0 12px 8px;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 1px;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
  }

  .project-group {
    margin-bottom: 2px;
  }

  .project-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px 0 12px;
  }

  .project-toggle {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    padding: 4px 0;
    cursor: pointer;
    color: inherit;
  }

  .project-toggle:hover .project-name {
    color: rgba(255, 255, 255, 0.9);
  }

  .chevron {
    display: flex;
    align-items: center;
    color: rgba(255, 255, 255, 0.3);
    transition: transform 0.15s ease;
    transform: rotate(0deg);
    width: 12px;
    flex-shrink: 0;
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .project-name {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.6);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  .action-btn {
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

  .action-btn:hover {
    color: rgba(255, 255, 255, 0.7);
    background: rgba(255, 255, 255, 0.08);
  }

  .detach-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.15);
    cursor: pointer;
    border-radius: 3px;
    transition:
      color 0.1s,
      background 0.1s;
  }

  .detach-btn:hover {
    color: rgba(255, 255, 255, 0.5);
    background: rgba(255, 255, 255, 0.06);
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
    padding: 4px 12px 4px 28px;
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

  .attach-btn {
    font-size: 10px;
    font-weight: 500;
    font-family: inherit;
    color: rgba(255, 255, 255, 0.25);
    background: none;
    border: none;
    padding: 4px 12px;
    margin-top: 4px;
    cursor: pointer;
    transition:
      color 0.1s,
      background 0.1s;
  }

  .attach-btn:hover {
    color: rgba(255, 255, 255, 0.6);
  }
</style>
