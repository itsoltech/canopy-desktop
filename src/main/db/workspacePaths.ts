/**
 * Canonical form for workspace/worktree paths persisted in the database or compared
 * across process boundaries: forward slashes only.
 *
 * Paths enter the app in both styles — native dialogs and Node APIs produce
 * `C:\source\repo` while git and the renderer produce `C:/source/repo`. Comparing
 * them as raw strings created duplicate workspace rows whose layouts could never be
 * cleaned up again (ghost windows re-spawned from them on every launch).
 */
export function normalizeWorkspacePath(path: string): string {
  return path.replace(/\\/g, '/')
}
