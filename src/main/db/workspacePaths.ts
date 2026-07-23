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

/**
 * Comparison form: normalized separators, case-folded on Windows (case-insensitive
 * filesystem — drive-letter/path casing routinely diverges between sources, e.g.
 * native dialog `C:\Source` vs msys `c:/source`). Follows the existing convention of
 * `comparableFsPath` (workspaceCommands) / `comparablePath` (handlers): fold on win32
 * only, never on case-sensitive filesystems. Use ONLY for comparisons and map/set
 * keys — the stored/displayed form stays case-preserved (`normalizeWorkspacePath`).
 */
export function comparableWorkspacePath(
  path: string,
  platform: NodeJS.Platform = process.platform,
): string {
  const normalized = normalizeWorkspacePath(path)
  return platform === 'win32' ? normalized.toLowerCase() : normalized
}
