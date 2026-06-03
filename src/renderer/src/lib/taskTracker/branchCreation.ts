import { workspaceState } from '../stores/workspace.svelte'
import { confirm } from '../stores/dialogs.svelte'

export async function createBranchFromTask(
  connectionId: string,
  task: TrackerTask,
): Promise<boolean> {
  const repoRoot = workspaceState.repoRoot
  const currentBranch = workspaceState.branch
  if (!repoRoot || !currentBranch) return false

  const { branchName } = await window.api.taskTrackerPrepareBranchFromTask({
    connectionId,
    task,
    repoRoot,
  })

  // Confirm with user
  const confirmed = await confirm({
    title: 'Create Branch',
    message: `Create and checkout branch from ${task.key}?`,
    details: branchName,
    confirmLabel: 'Create & Checkout',
  })
  if (!confirmed) return false

  const stashBeforeCreate = workspaceState.isDirty
  if (stashBeforeCreate) {
    const stash = await confirm({
      title: 'Uncommitted Changes',
      message: 'You have uncommitted changes. Stash them before switching?',
      confirmLabel: 'Stash & Continue',
    })
    if (!stash) return false
  }

  try {
    await window.api.taskTrackerCreateBranchFromTask({
      connectionId,
      task,
      repoRoot,
      baseBranch: currentBranch,
      stashBeforeCreate,
    })
  } catch (e) {
    await confirm({
      title: 'Branch Creation Failed',
      message: e instanceof Error ? e.message : 'Failed to create branch',
      confirmLabel: 'OK',
    })
    return false
  }

  return true
}
