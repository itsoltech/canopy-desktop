<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { ChevronRight, Square, Trash2, X } from '@lucide/svelte'
  import { fileManagerLabel } from '../../lib/platform'
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
  import { formatPrBadge, getBranchPRMap, loadBranchPRs } from '../../lib/stores/github.svelte'
  import { getResolvedConfig } from '../../lib/stores/taskTracker.svelte'

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
    await Promise.all(running.map((p) => window.api.killPty(p.sessionId, true)))
  }

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

  let collapseState: Record<string, boolean> = $state(
    Object.fromEntries(projects.map((p) => [p.workspace.path, isCollapsed(p)])),
  )

  $effect(() => {
    for (const p of projects) {
      if (!(p.workspace.path in collapseState)) {
        collapseState[p.workspace.path] = isCollapsed(p)
      }
    }
  })

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

  let prMap = $derived(getBranchPRMap())
  let githubConnectionCount = $derived(
    (getResolvedConfig()?.config.trackers ?? []).filter((t) => t.provider === 'github').length,
  )

  let prevGhConnCount = 0
  $effect(() => {
    const connCount = githubConnectionCount
    if (connCount === 0) return
    const deps = projects.map((p) => p.worktrees.length)
    void deps
    const force = connCount !== prevGhConnCount
    prevGhConnCount = connCount
    for (const p of projects) {
      if (p.isGitRepo && p.repoRoot) loadBranchPRs(p.repoRoot, force)
    }
  })

  const removingPaths = new SvelteSet<string>()

  async function doRemoveWorktree(
    project: ProjectState,
    wt: { path: string; branch: string },
  ): Promise<void> {
    if (!project.repoRoot || removingPaths.has(wt.path)) return
    removingPaths.add(wt.path)

    await closeAllTabsForWorktree(wt.path)

    const isDetached = wt.branch === '(detached)'
    try {
      await window.api.gitWorktreeRemove(project.repoRoot, wt.path, false)
      if (!isDetached) await window.api.gitBranchDelete(project.repoRoot, wt.branch, false)
    } catch {
      try {
        await window.api.gitWorktreeRemove(project.repoRoot, wt.path, true)
        if (!isDetached) await window.api.gitBranchDelete(project.repoRoot, wt.branch, true)
      } catch {
        removingPaths.delete(wt.path)
        return
      }
    }

    removingPaths.delete(wt.path)

    if (workspaceState.selectedWorktreePath === wt.path) {
      const main = project.worktrees.find((w) => w.isMain)
      if (main) selectWorktree(main.path)
    }
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

    await doRemoveWorktree(project, wt)
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

  function portal(node: HTMLElement): { destroy(): void } {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
  }

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
  }

  function closeCtxMenu(): void {
    ctxMenu = null
  }

  function handleCtxKeydown(e: KeyboardEvent): void {
    if (ctxMenu && e.key === 'Escape') {
      e.preventDefault()
      closeCtxMenu()
    }
  }

  async function ctxRevealInFileManager(): Promise<void> {
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
    await Promise.all(running.map((p) => window.api.killPty(p.sessionId, true)))
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

    await doRemoveWorktree(project, wt)
  }

  function statusDotBg(status: string): string {
    return (
      {
        idle: 'bg-success',
        working: 'bg-accent-text animate-badge-pulse motion-reduce:animate-none',
        waitingPermission: 'bg-warning-text animate-badge-pulse-fast motion-reduce:animate-none',
        error: 'bg-danger-text',
      }[status] ?? 'bg-text-faint'
    )
  }
</script>

<svelte:window onkeydown={handleCtxKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if ctxMenu}
  <div class="fixed inset-0 z-overlay" use:portal onclick={closeCtxMenu}>
    <div
      class="fixed min-w-45 bg-bg-overlay border border-border rounded-md shadow-ctx p-1 z-popover"
      style="left: {ctxMenu.x}px; top: {ctxMenu.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <button
        class="block w-full px-2.5 py-1.5 border-0 rounded-sm bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
        onclick={ctxRevealInFileManager}>{fileManagerLabel()}</button
      >
      <button
        class="block w-full px-2.5 py-1.5 border-0 rounded-sm bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
        onclick={ctxCopyPath}>Copy Path</button
      >
      {#if ctxMenu.wt.branch !== '(detached)'}
        <button
          class="block w-full px-2.5 py-1.5 border-0 rounded-sm bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
          onclick={ctxCopyBranch}>Copy Branch Name</button
        >
        <div class="h-px mx-2 my-1 bg-border-subtle"></div>
        <button
          class="block w-full px-2.5 py-1.5 border-0 rounded-sm bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
          onclick={ctxNewWorktree}>New Worktree from Branch</button
        >
      {/if}
      {#if isWorktreeActive(ctxMenu.wt.path)}
        <div class="h-px mx-2 my-1 bg-border-subtle"></div>
        <button
          class="block w-full px-2.5 py-1.5 border-0 rounded-sm bg-transparent text-danger-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
          onclick={ctxStopAll}>Stop All Terminals</button
        >
      {/if}
      {#if !ctxMenu.wt.isMain}
        <div class="h-px mx-2 my-1 bg-border-subtle"></div>
        <button
          class="block w-full px-2.5 py-1.5 border-0 rounded-sm bg-transparent text-danger-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
          onclick={ctxRemoveWorktree}>Remove Worktree</button
        >
      {/if}
    </div>
  </div>
{/if}

<section class="py-3">
  <div class="flex items-center justify-between px-3 h-7 mb-1">
    <h3 class="text-2xs font-semibold tracking-caps-looser uppercase text-text-faint leading-tight">
      PROJECTS
    </h3>
    <button
      class="inline-flex items-center h-5 px-1.5 rounded-sm font-inherit text-2xs font-medium text-text-faint bg-transparent border-0 cursor-pointer transition-colors duration-fast hover:text-text hover:bg-hover"
      onclick={handleAttachProject}
      title="Attach a project folder"
      aria-label="Attach a project folder"
    >
      + attach
    </button>
  </div>

  {#each projects as project (project.workspace.path)}
    {@const collapsed = collapseState[project.workspace.path] ?? false}
    {@const merged = getMerged(project)}
    <div class="mb-0.5">
      <div class="flex items-center justify-between pl-3 pr-2">
        <button
          class="group flex items-center gap-1 flex-1 min-w-0 bg-transparent border-0 py-1 cursor-pointer text-inherit rounded-sm"
          class:bg-active={!project.isGitRepo &&
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
            }
          }}
          aria-expanded={!collapsed}
        >
          <span
            class="flex items-center text-text-faint w-3 flex-shrink-0 transition-transform duration-base ease-std"
            class:rotate-90={!collapsed && project.isGitRepo}
          >
            {#if project.isGitRepo}
              <ChevronRight size={12} />
            {/if}
          </span>
          <span
            class="text-sm font-medium text-text-secondary group-hover:text-text overflow-hidden text-ellipsis whitespace-nowrap"
            >{project.workspace.name}</span
          >
        </button>
        <div class="flex items-center gap-0.5 flex-shrink-0">
          {#if project.isGitRepo}
            <button
              class="inline-flex items-center h-5 px-1.5 rounded-sm font-inherit text-2xs font-medium text-text-faint bg-transparent border-0 cursor-pointer transition-colors duration-fast hover:text-accent-text hover:bg-accent-bg"
              onclick={(e) => handleNewWorktree(e, project)}
              title="Create worktree">+ new</button
            >
          {:else}
            <button
              class="inline-flex items-center h-5 px-1.5 rounded-sm font-inherit text-2xs font-medium text-text-faint bg-transparent border-0 cursor-pointer transition-colors duration-fast hover:text-accent-text hover:bg-accent-bg"
              onclick={(e) => handleInitGit(e, project)}
              title="Initialize git repository">init</button
            >
          {/if}
          <button
            class="inline-flex items-center justify-center size-5 p-0 border-0 bg-transparent text-text-faint cursor-pointer rounded-sm transition-colors duration-fast hover:text-text hover:bg-hover"
            onclick={(e) => handleDetach(e, project)}
            title="Detach project from window"
            aria-label="Detach project from window"
          >
            <X size={10} />
          </button>
        </div>
      </div>

      {#if !collapsed && project.isGitRepo}
        <ul class="list-none p-0 m-0">
          {#each sortedWorktrees(project.worktrees) as wt (wt.path)}
            {@const wtActive = isWorktreeActive(wt.path)}
            {@const agentStatus = getWorktreeAgentStatus(wt.path)}
            {@const wtBadge = worktreeBadges[wt.path] ?? 'none'}
            {@const isRemoving = removingPaths.has(wt.path)}
            <li
              class="flex items-center"
              class:opacity-45={isRemoving}
              class:pointer-events-none={isRemoving}
            >
              <button
                class="flex items-center gap-1.5 flex-1 min-w-0 py-1 pr-2 pl-6 border-0 bg-transparent text-text-secondary text-sm font-inherit cursor-pointer text-left rounded-sm mx-1 hover:bg-hover hover:text-text"
                class:bg-active={wt.path === workspaceState.selectedWorktreePath}
                class:text-text={wt.path === workspaceState.selectedWorktreePath}
                onclick={() => selectWorktree(wt.path)}
                oncontextmenu={(e) => handleWorktreeContextMenu(e, project, wt)}
              >
                <span
                  class="font-mono text-xs text-text-secondary w-2.5 flex-shrink-0"
                  class:relative={agentStatus !== 'none'}
                  class:inline-flex={agentStatus !== 'none'}
                  class:items-center={agentStatus !== 'none'}
                  class:justify-center={agentStatus !== 'none'}
                  title={agentStatus !== 'none' ? `Agent: ${agentStatus}` : undefined}
                  aria-label={agentStatus !== 'none' ? `Agent status: ${agentStatus}` : undefined}
                >
                  {#if agentStatus !== 'none'}
                    <span
                      class="w-1.5 h-1.5 rounded-full {statusDotBg(agentStatus)}"
                      aria-hidden="true"
                    ></span>
                    {#if wtBadge !== 'none'}
                      <span
                        class="absolute -top-0.5 -right-0.5 w-1.25 h-1.25 rounded-full"
                        class:bg-accent-text={wtBadge !== 'permission'}
                        class:bg-warning-text={wtBadge === 'permission'}
                        class:animate-badge-pulse={wtBadge === 'permission'}
                        class:motion-reduce:animate-none={wtBadge === 'permission'}
                        aria-hidden="true"
                      ></span>
                    {/if}
                  {:else}
                    {wt.isMain ? '*' : ' '}
                  {/if}
                </span>
                <span
                  class="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap"
                  title={wt.path}>{worktreeLabel(wt)}</span
                >
                {#if isRemoving}
                  <span
                    class="inline-flex items-center h-4 px-1.5 rounded-md text-2xs font-semibold uppercase tracking-caps-tight bg-warning-bg text-warning-text leading-tight flex-shrink-0 animate-badge-pulse motion-reduce:animate-none"
                    >removing</span
                  >
                {:else if wt.branch === '(detached)'}
                  <span
                    class="inline-flex items-center h-4 px-1.5 rounded-md text-2xs font-semibold font-mono tracking-caps-tight bg-border-subtle text-warning-text leading-tight flex-shrink-0"
                    title={wt.head}>{wt.head.slice(0, 7)}</span
                  >
                {:else if merged.has(wt.branch)}
                  <span
                    class="inline-flex items-center h-4 px-1.5 rounded-md text-2xs font-semibold uppercase tracking-caps-tight bg-border-subtle text-success-text leading-tight flex-shrink-0"
                    title="Merged">merged</span
                  >
                {/if}
                {#if prMap[wt.branch]}
                  {@const pr = prMap[wt.branch]}
                  {@const badge = formatPrBadge(pr)}
                  <button
                    class={badge.className}
                    title={`${pr.title} — click to open`}
                    onclick={(e) => {
                      e.stopPropagation()
                      window.api.openExternal(pr.url)
                    }}
                  >
                    {badge.label}
                  </button>
                {/if}
                {#if wtActive}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <span
                    role="button"
                    tabindex="-1"
                    class="inline-flex items-center justify-center w-4 h-4 p-0 border-0 bg-transparent text-warning-text cursor-pointer flex-shrink-0 rounded-sm transition-colors duration-fast hover:text-danger hover:bg-danger-bg"
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
              {#if !wtActive && !wt.isMain && merged.has(wt.branch) && !isRemoving}
                <button
                  class="flex items-center justify-center w-6 h-6 p-0 border-0 bg-transparent text-text-faint cursor-pointer flex-shrink-0 rounded-md mr-1 transition-colors duration-fast hover:text-danger hover:bg-danger-bg"
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
</section>
