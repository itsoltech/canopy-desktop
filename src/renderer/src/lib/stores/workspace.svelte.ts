import { restoreLayout } from './tabs.svelte'

function basename(p: string): string {
  return p.split('/').pop() || p
}

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

interface AttachProjectOptions {
  selectIfEmpty?: boolean
  batchOrder?: Map<string, number>
  restoreIndex?: number
}

interface AttachProjectResult {
  projectPath: string
  defaultWorktreePath: string
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
}

/** Active selection — the currently focused project + worktree context */
export const workspaceState: WorkspaceState = $state({ ...initial })

/** All projects attached to this window */
export const projects: ProjectState[] = $state([])

// --- Multi-project functions ---

// Serialize concurrent attachProject calls to preserve project ordering
let attachQueue: Promise<void> = Promise.resolve()

export async function attachProject(path: string): Promise<void> {
  const result = attachQueue.then(() => attachProjectImpl(path))
  attachQueue = result.then(() => undefined).catch(() => {})
  await result
}

export async function restoreProjects(paths: string[], activeWorktreePath?: string): Promise<void> {
  const result = attachQueue.then(() => restoreProjectsImpl(paths, activeWorktreePath))
  attachQueue = result.then(() => undefined).catch(() => {})
  await result
}

function getProjectKey(project: ProjectState): string {
  return project.repoRoot ?? project.workspace.path
}

function getDefaultWorktreePath(project: ProjectState): string {
  if (!project.isGitRepo) return project.workspace.path
  const main = project.worktrees.find((wt) => wt.isMain)
  return main?.path ?? project.repoRoot ?? project.workspace.path
}

function reorderProjects(batchOrder: Map<string, number>): void {
  const ordered = [...projects]
    .map((project, index) => ({
      project,
      index,
      order: batchOrder.get(getProjectKey(project)),
    }))
    .sort((a, b) => {
      if (a.order === undefined && b.order === undefined) return a.index - b.index
      if (a.order === undefined) return 1
      if (b.order === undefined) return -1
      if (a.order !== b.order) return a.order - b.order
      return a.index - b.index
    })
    .map((entry) => entry.project)

  projects.splice(0, projects.length, ...ordered)
}

async function restoreProjectsImpl(paths: string[], activeWorktreePath?: string): Promise<void> {
  const uniquePaths = [...new Set(paths)]
  if (uniquePaths.length === 0) {
    if (activeWorktreePath) await selectWorktree(activeWorktreePath)
    return
  }

  const batchOrder = new Map<string, number>()
  const results = await Promise.all(
    uniquePaths.map((path, index) =>
      attachProjectImpl(path, {
        selectIfEmpty: false,
        batchOrder,
        restoreIndex: index,
      }),
    ),
  )

  let targetPath = activeWorktreePath
  if (!targetPath || !getProjectForWorktree(targetPath)) {
    targetPath = results.find((result) => result)?.defaultWorktreePath
  }

  if (targetPath) {
    await selectWorktree(targetPath)
  }
}

async function attachProjectImpl(
  path: string,
  options: AttachProjectOptions = {},
): Promise<AttachProjectResult | undefined> {
  const { selectIfEmpty = true, batchOrder, restoreIndex } = options
  // Dedupe: if another window already has this path, focus it instead
  const focused = await window.api.focusWindowForPath(path)
  if (focused) return

  // Detect git info
  const info: GitInfo = await window.api.gitDetect(path)
  const projectPath = info.repoRoot ?? path

  // Already attached in this window?
  if (projects.some((p) => (p.repoRoot ?? p.workspace.path) === projectPath)) {
    const existing = projects.find((p) => (p.repoRoot ?? p.workspace.path) === projectPath)
    if (!existing) return
    return {
      projectPath,
      defaultWorktreePath: getDefaultWorktreePath(existing),
    }
  }

  // Upsert workspace in DB
  const name = basename(projectPath)
  const ws = await window.api.upsertWorkspace({
    path: projectPath,
    name,
    isGitRepo: info.isGitRepo,
  })
  await window.api.touchWorkspace(ws.id)

  // Register with main process for dedup
  window.api.setWorkspacePath(projectPath)

  // Add to projects
  const project: ProjectState = {
    workspace: ws,
    isGitRepo: info.isGitRepo,
    repoRoot: info.repoRoot,
    worktrees: info.worktrees,
  }
  projects.push(project)
  if (batchOrder && restoreIndex !== undefined) {
    batchOrder.set(projectPath, restoreIndex)
    reorderProjects(batchOrder)
  }

  // Start git watcher if git repo
  if (info.isGitRepo && info.repoRoot) {
    await window.api.gitWatch(info.repoRoot, info)
  }

  // Restore saved layouts BEFORE selecting worktree so that ensureDefaultTab
  // (triggered by selectWorktree) finds existing tabs and doesn't spawn extras
  try {
    const layouts = await window.api.getAllLayouts(ws.id)
    if (layouts.length > 0) {
      for (const entry of layouts) {
        await restoreLayout(entry.worktree_path, entry.layout_json)
      }
    }
  } catch {
    // Layout restore failed, will fall back to ensureDefaultTab
  }

  // Auto-select if this is the first project or no active selection
  const defaultWorktreePath = getDefaultWorktreePath(project)

  if (selectIfEmpty && !workspaceState.selectedWorktreePath) {
    await selectWorktree(defaultWorktreePath)
  }

  return {
    projectPath,
    defaultWorktreePath,
  }
}

export async function detachProject(path: string): Promise<void> {
  const idx = projects.findIndex((p) => (p.repoRoot ?? p.workspace.path) === path)
  if (idx < 0) return

  const project = projects[idx]

  // Unwatch git
  if (project.isGitRepo && project.repoRoot) {
    await window.api.gitUnwatch(project.repoRoot)
  }

  // Unregister from main process
  await window.api.detachProject(path)

  // Remove from array
  projects.splice(idx, 1)

  // If active selection was in this project, fall back
  if (
    workspaceState.repoRoot === project.repoRoot ||
    workspaceState.workspace?.id === project.workspace.id
  ) {
    if (projects.length > 0) {
      const next = projects[0]
      if (next.isGitRepo) {
        const main = next.worktrees.find((wt) => wt.isMain)
        await selectWorktree(main?.path ?? next.repoRoot ?? next.workspace.path)
      } else {
        await selectWorktree(next.workspace.path)
      }
    } else {
      workspaceState.workspace = null
      workspaceState.isGitRepo = false
      workspaceState.repoRoot = null
      workspaceState.worktrees = []
      workspaceState.selectedWorktreePath = null
      workspaceState.branch = null
      workspaceState.isDirty = false
      workspaceState.aheadBehind = null
    }
  }
}

export async function initGitRepo(projectPath: string): Promise<void> {
  const project = projects.find((p) => p.workspace.path === projectPath)
  if (!project || project.isGitRepo) return

  const info: GitInfo = await window.api.gitInit(projectPath)
  project.isGitRepo = info.isGitRepo
  project.repoRoot = info.repoRoot
  project.worktrees = info.worktrees

  // Update workspace in DB
  const ws = await window.api.upsertWorkspace({
    path: info.repoRoot ?? projectPath,
    name: project.workspace.name,
    isGitRepo: info.isGitRepo,
  })
  project.workspace = ws

  // Start git watcher
  if (info.isGitRepo && info.repoRoot) {
    await window.api.gitWatch(info.repoRoot, info)
  }

  // If this project is the active selection, update workspaceState
  if (workspaceState.workspace?.id === ws.id) {
    workspaceState.isGitRepo = info.isGitRepo
    workspaceState.repoRoot = info.repoRoot
    workspaceState.worktrees = info.worktrees
    workspaceState.branch = info.branch
  }
}

/** Find which project owns a given worktree path */
export function getProjectForWorktree(wtPath: string): ProjectState | undefined {
  return projects.find(
    (p) =>
      p.worktrees.some((wt) => wt.path === wtPath) || (!p.isGitRepo && p.workspace.path === wtPath),
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
  // Find which project owns this worktree
  const project = getProjectForWorktree(path)
  if (project) {
    workspaceState.workspace = project.workspace
    workspaceState.isGitRepo = project.isGitRepo
    workspaceState.repoRoot = project.repoRoot
    workspaceState.worktrees = project.worktrees
  }

  window.api.setActiveWorktree(path)
  workspaceState.selectedWorktreePath = path

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

export { toggleFocusedInspector as toggleInspector } from './tabs.svelte'

export async function closeWorkspace(): Promise<void> {
  // Detach all projects
  const paths = projects.map((p) => p.repoRoot ?? p.workspace.path)
  for (const path of paths) {
    await detachProject(path)
  }
}
