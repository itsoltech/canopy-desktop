import { restoreLayout, closeAllTabsForWorktree, killAllTabs, tabsByWorktree } from './tabs.svelte'
import {
  loadRepoConfig,
  getRepoConfig,
  hasAnyCredentials,
  loadActiveTask,
} from './taskTracker.svelte'
import { addToast } from './toast.svelte'
import { clearQuickOpenCache } from './quickOpenStore.svelte'
import { clearMru } from './quickOpenMru.svelte'
import type {
  ProjectSnapshot,
  WorkspaceCommandResult,
  WorkspaceStateSnapshot,
} from '../../../../main/commands/types'

// --- Types ---

interface GitWorktreeInfo {
  path: string
  head: string
  branch: string
  isMain: boolean
  isBare: boolean
}

interface GitInfo {
  isGitRepo: boolean
  repoRoot: string | null
  branch: string | null
  worktrees: GitWorktreeInfo[]
  isDirty: boolean
  aheadBehind: { ahead: number; behind: number } | null
}

interface WorkspaceRow {
  id: string
  path: string
  name: string
  is_git_repo: number
  last_opened: string | null
  cached_branch: string | null
  cached_dirty: number | null
  cached_ahead_behind: string | null
  cached_worktree_count: number | null
}

export interface ProjectState {
  workspace: WorkspaceRow
  isGitRepo: boolean
  repoRoot: string | null
  worktrees: GitWorktreeInfo[]
}

interface WorkspaceState {
  workspace: WorkspaceRow | null
  isGitRepo: boolean
  repoRoot: string | null
  worktrees: GitWorktreeInfo[]
  selectedWorktreePath: string | null
  branch: string | null
  isDirty: boolean
  aheadBehind: { ahead: number; behind: number } | null
  sidebarOpen: boolean
  rightPanelOpen: boolean
  rightPanelTab: 'session' | 'changes'
  changesCount: number
  diffScrollTarget: { path: string; ts: number } | null
  diffVisibleFile: string | null
}

// --- State ---

const initial: WorkspaceState = {
  workspace: null,
  isGitRepo: false,
  repoRoot: null,
  worktrees: [],
  selectedWorktreePath: null,
  branch: null,
  isDirty: false,
  aheadBehind: null,
  sidebarOpen: true,
  rightPanelOpen: false,
  rightPanelTab: 'session',
  changesCount: 0,
  diffScrollTarget: null,
  diffVisibleFile: null,
}

/** Active selection — the currently focused project + worktree context */
export const workspaceState: WorkspaceState = $state({ ...initial })

/** All projects attached to this window */
export const projects: ProjectState[] = $state([])

// --- Multi-project functions ---

// Serialize concurrent attachProject calls to preserve project ordering
let attachQueue: Promise<void> = Promise.resolve()

export async function attachProject(path: string): Promise<void> {
  const result = attachQueue.then(async () => {
    const commandResult = await window.api.workspaceAttachProject(path)
    await applyWorkspaceCommandResult(commandResult)
    if (commandResult.workspaceState.selectedWorktreePath) {
      await hydrateSelectedWorktree(commandResult.workspaceState.selectedWorktreePath)
    }
  })
  attachQueue = result
    .then(() => undefined)
    .catch((err) => {
      console.error('[workspace] attachProject queue error:', err)
    })
  await result
}

export async function restoreProjects(
  paths: string[],
  activeWorktreePath?: string,
  removedPaths?: string[],
): Promise<void> {
  const result = attachQueue.then(async () => {
    const commandResult = await window.api.workspaceRestoreWindow({
      paths,
      activeWorktreePath,
      removedPaths,
    })
    await applyWorkspaceCommandResult(commandResult)
    if (commandResult.workspaceState.selectedWorktreePath) {
      await hydrateSelectedWorktree(commandResult.workspaceState.selectedWorktreePath)
    }
  })
  attachQueue = result
    .then(() => undefined)
    .catch((err) => {
      console.error('[workspace] restoreProjects queue error:', err)
    })
  await result
}

function getProjectKey(project: ProjectState): string {
  return project.repoRoot ?? project.workspace.path
}

function projectFromSnapshot(project: ProjectSnapshot): ProjectState {
  return {
    workspace: project.workspace,
    isGitRepo: project.isGitRepo,
    repoRoot: project.repoRoot,
    worktrees: project.worktrees,
  }
}

function applyWorkspaceStateSnapshot(snapshot: WorkspaceStateSnapshot): void {
  if (!snapshot.project) {
    workspaceState.workspace = null
    workspaceState.isGitRepo = false
    workspaceState.repoRoot = null
    workspaceState.worktrees = []
    workspaceState.selectedWorktreePath = snapshot.selectedWorktreePath
    workspaceState.branch = snapshot.branch
    workspaceState.isDirty = snapshot.isDirty
    workspaceState.aheadBehind = snapshot.aheadBehind
    return
  }

  const key = snapshot.project.repoRoot ?? snapshot.project.workspace.path
  const project = projects.find((candidate) => getProjectKey(candidate) === key)

  workspaceState.workspace = project?.workspace ?? snapshot.project.workspace
  workspaceState.isGitRepo = project?.isGitRepo ?? snapshot.project.isGitRepo
  workspaceState.repoRoot = project?.repoRoot ?? snapshot.project.repoRoot
  workspaceState.worktrees = project?.worktrees ?? snapshot.project.worktrees
  workspaceState.selectedWorktreePath = snapshot.selectedWorktreePath
  workspaceState.branch = snapshot.branch
  workspaceState.isDirty = snapshot.isDirty
  workspaceState.aheadBehind = snapshot.aheadBehind
}

async function loadRepoConfigsForNewProjects(existingKeys: Set<string>): Promise<void> {
  for (const project of projects) {
    const projectKey = getProjectKey(project)
    if (existingKeys.has(projectKey) || !project.repoRoot) continue

    try {
      await loadRepoConfig(project.repoRoot)
      if (getRepoConfig()?.trackers.length && !hasAnyCredentials()) {
        addToast('Tracker requires authentication - configure token in Preferences')
      }
    } catch (err) {
      console.error(`[workspace] loadRepoConfig failed for "${projectKey}":`, err)
    }
  }
}

async function restoreCommandLayouts(result: WorkspaceCommandResult): Promise<void> {
  for (const entry of result.restoredLayouts ?? []) {
    try {
      await restoreLayout(entry.worktreePath, entry.layoutJson)
    } catch {
      // Layout restore failed, will fall back to ensureDefaultTab.
    }
  }
}

async function applyWorkspaceCommandResult(
  result: WorkspaceCommandResult,
  options: { restoreLayouts?: boolean; loadNewProjectConfigs?: boolean } = {},
): Promise<void> {
  const { restoreLayouts = true, loadNewProjectConfigs = true } = options
  const existingKeys = new Set(projects.map(getProjectKey))

  for (const warning of result.warnings) {
    addToast(warning.message)
  }

  projects.splice(0, projects.length, ...result.projects.map(projectFromSnapshot))

  if (restoreLayouts) {
    await restoreCommandLayouts(result)
  }

  applyWorkspaceStateSnapshot(result.workspaceState)

  if (loadNewProjectConfigs) {
    await loadRepoConfigsForNewProjects(existingKeys)
  }
}

export async function detachProject(path: string): Promise<void> {
  const idx = projects.findIndex((p) => (p.repoRoot ?? p.workspace.path) === path)
  if (idx < 0) return

  const project = projects[idx]
  console.warn(`[workspace] detaching "${project.workspace.name}" (${path})`)

  // Close tabs for every path owned by this project (kills PTY sessions).
  // Includes workspace path, repoRoot, all worktree paths, plus any
  // tabsByWorktree key under the repo root — so nothing lingers under a
  // stale or symlink-normalized key.
  const ownedPaths: string[] = []
  const addPath = (p: string | null | undefined): void => {
    if (p && !ownedPaths.includes(p)) ownedPaths.push(p)
  }
  addPath(project.workspace.path)
  addPath(project.repoRoot)
  for (const wt of project.worktrees) addPath(wt.path)
  if (project.repoRoot) {
    const normalize = (s: string): string => s.replace(/\\/g, '/')
    const rootNorm = normalize(project.repoRoot) + '/'
    for (const key of Object.keys(tabsByWorktree)) {
      if (normalize(key).startsWith(rootNorm)) addPath(key)
    }
  }
  for (const wtPath of ownedPaths) {
    await closeAllTabsForWorktree(wtPath)
    clearQuickOpenCache(wtPath)
    clearMru(wtPath)
  }

  const wasActive =
    workspaceState.repoRoot === project.repoRoot ||
    workspaceState.workspace?.id === project.workspace.id
  const commandResult = await window.api.workspaceDetachProject(path)

  // Remove from array
  projects.splice(idx, 1)
  await applyWorkspaceCommandResult(commandResult, {
    restoreLayouts: false,
    loadNewProjectConfigs: false,
  })

  // Main owns fallback selection; renderer performs UI-only side effects for the returned selection.
  if (wasActive) {
    if (workspaceState.selectedWorktreePath) {
      await hydrateSelectedWorktree(workspaceState.selectedWorktreePath)
    } else {
      workspaceState.workspace = null
      workspaceState.isGitRepo = false
      workspaceState.repoRoot = null
      workspaceState.worktrees = []
      workspaceState.selectedWorktreePath = null
      workspaceState.branch = null
      workspaceState.isDirty = false
      workspaceState.aheadBehind = null

      // No projects left — stop the file tree watcher
      await window.api.unwatchFiles()
    }
  }

  // Safety net: if no projects remain, kill any remaining PTYs/tabs that
  // might have been registered under stale keys, so the dashboard can render.
  if (projects.length === 0 && Object.keys(tabsByWorktree).length > 0) {
    await killAllTabs()
  }
}

export async function initGitRepo(projectPath: string): Promise<void> {
  const project = projects.find((p) => p.workspace.path === projectPath)
  if (!project || project.isGitRepo) return

  const commandResult = await window.api.workspaceInitGitRepo(projectPath)
  await applyWorkspaceCommandResult(commandResult, {
    restoreLayouts: false,
    loadNewProjectConfigs: false,
  })
  if (commandResult.workspaceState.selectedWorktreePath) {
    await hydrateSelectedWorktree(commandResult.workspaceState.selectedWorktreePath)
  }
}

/** Find which project owns a given worktree path */
export function getProjectForWorktree(wtPath: string): ProjectState | undefined {
  return projects.find(
    (p) =>
      p.worktrees.some((wt) => wt.path === wtPath) ||
      p.repoRoot === wtPath ||
      p.workspace.path === wtPath,
  )
}

export async function updateGitInfoForProject(repoRoot: string, info: GitInfo): Promise<void> {
  const project = projects.find((p) => p.repoRoot === repoRoot)
  if (!project) return

  project.worktrees = info.worktrees

  // If the active selection is in this project, update workspaceState too
  const isActive = workspaceState.repoRoot === repoRoot
  if (isActive) {
    workspaceState.worktrees = info.worktrees

    // If the selected worktree was removed, fall back to main
    if (workspaceState.selectedWorktreePath) {
      const still = info.worktrees.find((wt) => wt.path === workspaceState.selectedWorktreePath)
      if (!still) {
        const main = info.worktrees.find((wt) => wt.isMain)
        workspaceState.selectedWorktreePath = main?.path ?? null
        workspaceState.branch = main?.branch ?? info.branch
      }
    }

    // Fetch status for the currently selected worktree
    const selectedPath = workspaceState.selectedWorktreePath
    if (selectedPath) {
      const mainWorktreePath =
        info.worktrees.find((wt) => wt.isMain)?.path ?? project.repoRoot ?? project.workspace.path

      if (selectedPath === mainWorktreePath) {
        workspaceState.branch = info.branch ?? workspaceState.branch
        workspaceState.isDirty = info.isDirty
        workspaceState.aheadBehind = info.aheadBehind
      } else {
        const status = await window.api.gitStatus(selectedPath)
        workspaceState.branch = status.branch ?? workspaceState.branch
        workspaceState.isDirty = status.isDirty
        workspaceState.aheadBehind = status.aheadBehind
      }
    } else {
      workspaceState.branch = info.branch
      workspaceState.isDirty = info.isDirty
      workspaceState.aheadBehind = info.aheadBehind
    }
  }
}

// --- Existing functions (updated for multi-project) ---

/** openWorkspace is kept as a thin wrapper for backward compat (WelcomeDashboard, CommandPalette) */
export async function openWorkspace(path: string): Promise<void> {
  await attachProject(path)
}

export async function selectWorktree(path: string): Promise<void> {
  const commandResult = await window.api.workspaceSelectWorktree(path)
  await applyWorkspaceCommandResult(commandResult, {
    restoreLayouts: false,
    loadNewProjectConfigs: false,
  })
  await hydrateSelectedWorktree(path)
}

async function hydrateSelectedWorktree(path: string): Promise<void> {
  await loadActiveTask(path)

  // Start (or restart) the file tree watcher for the newly active worktree.
  // The main process disposes any previous watcher for this window before
  // starting a new one, so we don't need an explicit unwatch here.
  try {
    await window.api.watchFiles(path)
  } catch (err) {
    console.error(`[workspace] watchFiles failed for "${path}":`, err)
  }

  const project = getProjectForWorktree(path)
  if (project?.isGitRepo) {
    const wt = project.worktrees.find((w) => w.path === path)
    if (wt) {
      workspaceState.branch = wt.branch
    }

    // Fetch per-worktree git status
    const status = await window.api.gitStatus(path)
    workspaceState.isDirty = status.isDirty
    workspaceState.aheadBehind = status.aheadBehind
  } else {
    workspaceState.branch = null
    workspaceState.isDirty = false
    workspaceState.aheadBehind = null
  }
}

/** @deprecated Use updateGitInfoForProject instead */
export async function updateGitInfo(info: GitInfo & { repoRoot?: string }): Promise<void> {
  if (info.repoRoot) {
    await updateGitInfoForProject(info.repoRoot, info)
  }
}

export function toggleSidebar(): void {
  workspaceState.sidebarOpen = !workspaceState.sidebarOpen
}

export function toggleRightPanel(): void {
  workspaceState.rightPanelOpen = !workspaceState.rightPanelOpen
}

export async function closeWorkspace(): Promise<void> {
  // Detach all projects
  const paths = projects.map((p) => p.repoRoot ?? p.workspace.path)
  for (const path of paths) {
    await detachProject(path)
  }
}
