<script lang="ts">
  import { ChevronRight, Square, Trash2, X } from '@lucide/svelte'
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
  import { getTabsForWorktree, closeAllTabsForWorktree } from '../../lib/stores/tabs.svelte'
  import { allPanes } from '../../lib/stores/splitTree'
  import { worktreeBadges } from '../../lib/agents/agentState.svelte'
  import { getWorktreeAgentStatus } from '../../lib/agents/worktreeStatus.svelte'

  function worktreeLabel(wt: { branch: string; path: string }): string {
    if (wt.branch !== '(detached)') return wt.branch
    return wt.path.split('/').pop() || wt.path
  }

  function isWorktreeActive(worktreePath: string): boolean {
    const tabs = getTabsForWorktree(worktreePath)
    return tabs.some((t) => allPanes(t.rootSplit).some((p) => p.isRunning))
  }

  function sortedWorktrees(worktrees: ProjectState['worktrees']): ProjectState['worktrees'] {
    return [...worktrees].sort((a, b) => {
      if (a.isMain !== b.isMain) return a.isMain ? -1 : 1
      const aActive = isWorktreeActive(a.path)
      const bActive = isWorktreeActive(b.path)
      if (aActive !== bActive) return aActive ? -1 : 1
      return 0
    })
  }

  async function stopWorktree(e: MouseEvent, worktreePath: string): Promise<void> {
    e.stopPropagation()
    const tabs = getTabsForWorktree(worktreePath)
    const panes = tabs.flatMap((t) => allPanes(t.rootSplit))
    const running = panes.filter((p) => p.isRunning)
    if (running.length === 0) return
    await Promise.all(running.map((p) => window.api.killPty(p.sessionId)))
  }

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

  async function checkMergedStatus(project: ProjectState, signal: AbortSignal): Promise<void> {
    if (!project.isGitRepo || !project.repoRoot) return
    const checks = project.worktrees
      .filter((wt) => !wt.isMain)
      .map(async (wt) => {
        const merged = await window.api.gitBranchMerged(project.repoRoot!, wt.branch)
        return { branch: wt.branch, merged }
      })
    const results = await Promise.all(checks)
    if (signal.aborted) return
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- temporary local Set, not reactive state
    const next = new Set<string>()
    for (const r of results) {
      if (r.merged) next.add(r.branch)
    }
    mergedBranches = { ...mergedBranches, [project.workspace.path]: next }
  }

  // Re-check merged status when any project's worktrees change
  $effect(() => {
    const ac = new AbortController()
    const deps = projects.map((p) => p.worktrees.length)
    if (deps.length >= 0) {
      for (const p of projects) {
        checkMergedStatus(p, ac.signal)
      }
    }
    return () => ac.abort()
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

    await closeAllTabsForWorktree(wt.path)

    try {
      await window.api.gitWorktreeRemove(project.repoRoot, wt.path, false)
      if (!isDetached) await window.api.gitBranchDelete(project.repoRoot, wt.branch, false)
    } catch {
      try {
        await window.api.gitWorktreeRemove(project.repoRoot, wt.path, true)
        if (!isDetached) await window.api.gitBranchDelete(project.repoRoot, wt.branch, true)
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

  // Move element to document.body to escape sidebar overflow/backdrop-filter
  function portal(node: HTMLElement): { destroy(): void } {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
  }

  // --- Worktree context menu ---

  interface WorktreeCtx {
    x: number
    y: number
    project: ProjectState
    wt: ProjectState['worktrees'][number]
  }

  let ctxMenu = $state<WorktreeCtx | null>(null)

  function handleWorktreeContextMenu(
    e: MouseEvent,
    project: ProjectState,
    wt: ProjectState['worktrees'][number],
  ): void {
    e.preventDefault()
    ctxMenu = { x: e.clientX, y: e.clientY, project, wt }
    window.dispatchEvent(new CustomEvent('canopy:freeze-browsers'))
  }

  function closeCtxMenu(): void {
    ctxMenu = null
    window.dispatchEvent(new CustomEvent('canopy:unfreeze-browsers'))
  }

  function handleCtxKeydown(e: KeyboardEvent): void {
    if (ctxMenu && e.key === 'Escape') {
      e.preventDefault()
      closeCtxMenu()
    }
  }

  async function ctxShowInFinder(): Promise<void> {
    if (!ctxMenu) return
    await window.api.showInFolder(ctxMenu.wt.path)
    closeCtxMenu()
  }

  async function ctxCopyPath(): Promise<void> {
    if (!ctxMenu) return
    await navigator.clipboard.writeText(ctxMenu.wt.path)
    closeCtxMenu()
  }

  async function ctxCopyBranch(): Promise<void> {
    if (!ctxMenu) return
    await navigator.clipboard.writeText(ctxMenu.wt.branch)
    closeCtxMenu()
  }

  function ctxNewWorktree(): void {
    if (!ctxMenu) return
    const { project, wt } = ctxMenu
    closeCtxMenu()
    showCreateWorktree({
      repoRoot: project.repoRoot!,
      workspaceId: project.workspace.id,
      baseBranch: wt.branch,
    })
  }

  async function ctxStopAll(): Promise<void> {
    if (!ctxMenu) return
    const tabs = getTabsForWorktree(ctxMenu.wt.path)
    const running = tabs.flatMap((t) => allPanes(t.rootSplit)).filter((p) => p.isRunning)
    await Promise.all(running.map((p) => window.api.killPty(p.sessionId)))
    closeCtxMenu()
  }

  async function ctxRemoveWorktree(): Promise<void> {
    if (!ctxMenu) return
    const { project, wt } = ctxMenu
    closeCtxMenu()
    if (!project.repoRoot) return

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

    await closeAllTabsForWorktree(wt.path)

    try {
      await window.api.gitWorktreeRemove(project.repoRoot, wt.path, false)
      if (!isDetached) await window.api.gitBranchDelete(project.repoRoot, wt.branch, false)
    } catch {
      try {
        await window.api.gitWorktreeRemove(project.repoRoot, wt.path, true)
        if (!isDetached) await window.api.gitBranchDelete(project.repoRoot, wt.branch, true)
      } catch {
        // Ignore — watcher will update the list
      }
    }

    if (workspaceState.selectedWorktreePath === wt.path) {
      const main = project.worktrees.find((w) => w.isMain)
      if (main) selectWorktree(main.path)
    }
  }
</script>

<svelte:window onkeydown={handleCtxKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if ctxMenu}
  <div class="ctx-overlay" use:portal onclick={closeCtxMenu}>
    <div
      class="ctx-menu"
      style="left: {ctxMenu.x}px; top: {ctxMenu.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <button class="ctx-item" onclick={ctxShowInFinder}>Show in Finder</button>
      <button class="ctx-item" onclick={ctxCopyPath}>Copy Path</button>
      {#if ctxMenu.wt.branch !== '(detached)'}
        <button class="ctx-item" onclick={ctxCopyBranch}>Copy Branch Name</button>
        <div class="ctx-divider"></div>
        <button class="ctx-item" onclick={ctxNewWorktree}>New Worktree from Branch</button>
      {/if}
      {#if isWorktreeActive(ctxMenu.wt.path)}
        <div class="ctx-divider"></div>
        <button class="ctx-item destructive" onclick={ctxStopAll}>Stop All Terminals</button>
      {/if}
      {#if !ctxMenu.wt.isMain}
        <div class="ctx-divider"></div>
        <button class="ctx-item destructive" onclick={ctxRemoveWorktree}>Remove Worktree</button>
      {/if}
    </div>
  </div>
{/if}

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
          class:active={!project.isGitRepo &&
            workspaceState.selectedWorktreePath === project.workspace.path}
          onclick={() => {
            if (project.isGitRepo) {
              toggleCollapse(project)
            } else {
              selectWorktree(project.workspace.path)
            }
          }}
          oncontextmenu={(e) => {
            if (!project.isGitRepo) {
              e.preventDefault()
              ctxMenu = {
                x: e.clientX,
                y: e.clientY,
                project,
                wt: {
                  path: project.workspace.path,
                  head: '',
                  branch: '',
                  isMain: true,
                  isBare: false,
                },
              }
              window.dispatchEvent(new CustomEvent('canopy:freeze-browsers'))
            }
          }}
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
            aria-label="Detach project from window"
          >
            <X size={10} />
          </button>
        </div>
      </div>

      {#if !collapsed && project.isGitRepo}
        <ul class="worktree-list">
          {#each sortedWorktrees(project.worktrees) as wt (wt.path)}
            {@const wtActive = isWorktreeActive(wt.path)}
            {@const agentStatus = getWorktreeAgentStatus(wt.path)}
            {@const wtBadge = worktreeBadges[wt.path] ?? 'none'}
            <li class="worktree-row">
              <button
                class="worktree-item"
                class:active={wt.path === workspaceState.selectedWorktreePath}
                onclick={() => selectWorktree(wt.path)}
                oncontextmenu={(e) => handleWorktreeContextMenu(e, project, wt)}
              >
                <span
                  class="indicator"
                  class:has-dot={agentStatus !== 'none'}
                  title={agentStatus !== 'none' ? `Agent: ${agentStatus}` : undefined}
                  aria-label={agentStatus !== 'none' ? `Agent status: ${agentStatus}` : undefined}
                >
                  {#if agentStatus !== 'none'}
                    <span class="wt-status-dot {agentStatus}" aria-hidden="true"></span>
                    {#if wtBadge !== 'none'}
                      <span
                        class="wt-notify-dot"
                        class:permission={wtBadge === 'permission'}
                        aria-hidden="true"
                      ></span>
                    {/if}
                  {:else}
                    {wt.isMain ? '*' : ' '}
                  {/if}
                </span>
                <span class="branch-name" title={wt.path}>{worktreeLabel(wt)}</span>
                {#if wt.branch === '(detached)'}
                  <span class="detached-badge" title={wt.head}>{wt.head.slice(0, 7)}</span>
                {:else if merged.has(wt.branch)}
                  <span class="merged-badge" title="Merged">merged</span>
                {/if}
                {#if wtActive}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <span
                    role="button"
                    tabindex="-1"
                    class="stop-btn"
                    title="Stop all terminals in this worktree"
                    onclick={(e) => {
                      e.stopPropagation()
                      stopWorktree(e, wt.path)
                    }}
                  >
                    <Square size={8} />
                  </span>
                {/if}
              </button>
              {#if !wtActive && !wt.isMain && merged.has(wt.branch)}
                <button
                  class="remove-btn"
                  title="Remove worktree and delete branch"
                  aria-label="Remove worktree and delete branch"
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

  .project-toggle.active {
    background: rgba(255, 255, 255, 0.1);
    border-radius: 4px;
  }

  .project-toggle.active .project-name {
    color: #fff;
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
    padding: 4px 8px 4px 28px;
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

  .indicator.has-dot {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .wt-status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.2);
  }

  .wt-status-dot.idle {
    background: rgba(100, 200, 100, 0.6);
  }

  .wt-status-dot.working {
    background: rgba(116, 192, 252, 0.8);
    animation: wt-pulse 1.5s ease-in-out infinite;
  }

  .wt-status-dot.waitingPermission {
    background: rgba(255, 160, 50, 0.9);
    animation: wt-pulse 1s ease-in-out infinite;
  }

  .wt-status-dot.error {
    background: rgba(255, 100, 100, 0.8);
  }

  .wt-notify-dot {
    position: absolute;
    top: -2px;
    right: -2px;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: rgba(116, 192, 252, 0.8);
  }

  .wt-notify-dot.permission {
    background: rgba(255, 160, 50, 0.9);
    animation: wt-pulse 1.5s ease-in-out infinite;
  }

  @keyframes wt-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .wt-status-dot.working,
    .wt-status-dot.waitingPermission,
    .wt-notify-dot.permission {
      animation: none;
    }
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

  .stop-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: none;
    background: none;
    color: rgba(255, 180, 80, 0.5);
    cursor: pointer;
    flex-shrink: 0;
    margin-left: auto;
    transition:
      color 0.1s,
      background 0.1s;
  }

  .stop-btn:hover {
    color: #e05050;
    background: rgba(224, 80, 80, 0.12);
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

  .ctx-overlay {
    position: fixed;
    inset: 0;
    z-index: 1002;
  }

  .ctx-menu {
    position: fixed;
    min-width: 180px;
    background: rgba(40, 40, 40, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    padding: 4px;
    z-index: 1003;
  }

  .ctx-item {
    display: block;
    width: 100%;
    padding: 6px 12px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.8);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.05s;
  }

  .ctx-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .ctx-item.destructive {
    color: rgba(255, 120, 120, 0.9);
  }

  .ctx-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 4px 8px;
  }
</style>
