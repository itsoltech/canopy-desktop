import { restoreLayout, saveAllLayouts } from './tabs.svelte'

function basename(p: string): string {
  return p.split('/').pop() || p
}

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
  is_pinned: number
  last_opened: string | null
  cached_branch: string | null
  cached_dirty: number | null
  cached_ahead_behind: string | null
  cached_worktree_count: number | null
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
  inspectorOpen: boolean
}

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
  inspectorOpen: false,
}

export const workspaceState: WorkspaceState = $state({ ...initial })

const worktreeCache = new Map<string, GitWorktreeInfo[]>()

export const projectList: WorkspaceRow[] = $state([])

export async function loadProjectList(): Promise<void> {
  const rows = await window.api.listAllWorkspaces()
  projectList.length = 0
  projectList.push(...rows)
}

export async function openWorkspace(path: string, skipFocusCheck = false): Promise<void> {
  // Dedupe: if another window already has this path, focus it instead
  if (!skipFocusCheck) {
    const focused = await window.api.focusWindowForPath(path)
    if (focused) return
  }

  // Detect git info
  const info: GitInfo = await window.api.gitDetect(path)

  // Upsert workspace in DB
  const name = basename(info.repoRoot ?? path)
  const ws = await window.api.upsertWorkspace({
    path: info.repoRoot ?? path,
    name,
    isGitRepo: info.isGitRepo,
  })

  // Touch to update last_opened
  await window.api.touchWorkspace(ws.id)

  // Register workspace path with main process for dedup
  window.api.setWorkspacePath(info.repoRoot ?? path)

  // Update state
  workspaceState.workspace = ws
  workspaceState.isGitRepo = info.isGitRepo
  workspaceState.repoRoot = info.repoRoot
  workspaceState.worktrees = info.worktrees
  workspaceState.branch = info.branch
  workspaceState.isDirty = info.isDirty
  workspaceState.aheadBehind = info.aheadBehind

  if (info.isGitRepo && info.repoRoot) {
    // Select the main worktree by default
    const main = info.worktrees.find((wt) => wt.isMain)
    workspaceState.selectedWorktreePath = main?.path ?? info.repoRoot

    // Start watching for git changes
    await window.api.gitWatch(info.repoRoot)
  } else {
    // Non-git folder: use the path itself
    workspaceState.selectedWorktreePath = path
  }

  // Restore saved layouts for this workspace
  try {
    const layouts = await window.api.getAllLayouts(ws.id)
    if (layouts.length > 0) {
      for (const entry of layouts) {
        await restoreLayout(entry.worktree_path, entry.layout_json)
      }
      // Mark restore complete — MainLayout's ensureShellTab will skip worktrees with tabs
    }
  } catch {
    // Layout restore failed, will fall back to ensureShellTab
  }
}

export async function selectWorktree(path: string): Promise<void> {
  workspaceState.selectedWorktreePath = path
  const wt = workspaceState.worktrees.find((w) => w.path === path)
  if (wt) {
    workspaceState.branch = wt.branch
  }

  // Fetch per-worktree git status
  const status = await window.api.gitStatus(path)
  workspaceState.isDirty = status.isDirty
  workspaceState.aheadBehind = status.aheadBehind
}

export async function updateGitInfo(info: GitInfo): Promise<void> {
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

  // Fetch status for the currently selected worktree (not the main one)
  const selectedPath = workspaceState.selectedWorktreePath
  if (selectedPath) {
    const status = await window.api.gitStatus(selectedPath)
    workspaceState.branch = status.branch ?? workspaceState.branch
    workspaceState.isDirty = status.isDirty
    workspaceState.aheadBehind = status.aheadBehind
  } else {
    workspaceState.branch = info.branch
    workspaceState.isDirty = info.isDirty
    workspaceState.aheadBehind = info.aheadBehind
  }
}

export function toggleSidebar(): void {
  workspaceState.sidebarOpen = !workspaceState.sidebarOpen
}

export function toggleInspector(): void {
  workspaceState.inspectorOpen = !workspaceState.inspectorOpen
}

export async function closeWorkspace(): Promise<void> {
  if (workspaceState.repoRoot) {
    await window.api.gitUnwatch()
  }
  Object.assign(workspaceState, { ...initial })
}

export async function switchProject(path: string): Promise<void> {
  // Skip if already on this project
  if (workspaceState.workspace?.path === path) return

  // 1. Save layouts with current workspace ID before switching
  saveAllLayouts()

  // 2. Stop git watcher for old project (fire-and-forget)
  if (workspaceState.repoRoot) {
    window.api.gitUnwatch()
  }

  // 3. Cache current worktrees before switching away
  if (workspaceState.workspace?.path && workspaceState.worktrees.length > 0) {
    worktreeCache.set(workspaceState.workspace.path, workspaceState.worktrees)
  }

  // 4. Find cached workspace data from projectList
  const cached = projectList.find((ws) => ws.path === path)

  if (!cached) {
    // First-time open (e.g. from "+ open" button) — use full openWorkspace
    await openWorkspace(path, true)
    await loadProjectList()
    return
  }

  // 4. Instantly update UI with cached data
  workspaceState.workspace = cached
  workspaceState.isGitRepo = cached.is_git_repo === 1
  workspaceState.repoRoot = cached.is_git_repo ? cached.path : null
  workspaceState.branch = cached.cached_branch
  workspaceState.isDirty = cached.cached_dirty === 1
  workspaceState.aheadBehind = cached.cached_ahead_behind
    ? JSON.parse(cached.cached_ahead_behind)
    : null
  workspaceState.selectedWorktreePath = cached.path
  workspaceState.worktrees = worktreeCache.get(path) ?? []

  // 5. Register with main process + touch (fire-and-forget)
  window.api.setWorkspacePath(path)
  window.api.touchWorkspace(cached.id)

  // 6. Restore layouts — skips if tabs already exist (live sessions guard)
  try {
    const layouts = await window.api.getAllLayouts(cached.id)
    for (const entry of layouts) {
      await restoreLayout(entry.worktree_path, entry.layout_json)
    }
  } catch {
    // Layout restore failed, will fall back to ensureShellTab
  }

  // 7. Background: full git detect to fill in worktrees + start watcher
  if (cached.is_git_repo) {
    window.api.gitDetect(path).then(async (info) => {
      // Only update if still on this project
      if (workspaceState.workspace?.path !== path) return
      worktreeCache.set(path, info.worktrees)
      workspaceState.worktrees = info.worktrees
      workspaceState.branch = info.branch
      workspaceState.isDirty = info.isDirty
      workspaceState.aheadBehind = info.aheadBehind
      workspaceState.repoRoot = info.repoRoot
      if (info.repoRoot) {
        const main = info.worktrees.find((wt) => wt.isMain)
        workspaceState.selectedWorktreePath = main?.path ?? info.repoRoot
        await window.api.gitWatch(info.repoRoot)
      }
    })
  }

  // 8. Refresh project list (fire-and-forget)
  loadProjectList()
}

export async function toggleProjectPin(id: string): Promise<void> {
  await window.api.togglePinWorkspace(id)
  await loadProjectList()
}

export async function removeProject(id: string): Promise<void> {
  await window.api.removeWorkspace(id)
  await loadProjectList()
}
