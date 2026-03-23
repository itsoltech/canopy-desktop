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

export async function openWorkspace(path: string): Promise<void> {
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
}

export function selectWorktree(path: string): void {
  workspaceState.selectedWorktreePath = path
  const wt = workspaceState.worktrees.find((w) => w.path === path)
  if (wt) {
    workspaceState.branch = wt.branch
  }
}

export function updateGitInfo(info: GitInfo): void {
  workspaceState.worktrees = info.worktrees
  workspaceState.branch = info.branch
  workspaceState.isDirty = info.isDirty
  workspaceState.aheadBehind = info.aheadBehind

  // If the selected worktree was removed, fall back to main
  if (workspaceState.selectedWorktreePath) {
    const still = info.worktrees.find((wt) => wt.path === workspaceState.selectedWorktreePath)
    if (!still) {
      const main = info.worktrees.find((wt) => wt.isMain)
      workspaceState.selectedWorktreePath = main?.path ?? null
      workspaceState.branch = main?.branch ?? info.branch
    }
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
