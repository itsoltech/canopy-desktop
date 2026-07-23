/**
 * Canonical form for workspace/worktree paths persisted in the database or compared
 * across process boundaries. On Windows, separators fold to forward slashes: paths
 * enter the app in both styles — native dialogs and Node APIs produce
 * `C:\source\repo` while git and the renderer produce `C:/source/repo` — and
 * comparing them as raw strings created duplicate workspace rows whose layouts could
 * never be cleaned up again (ghost windows re-spawned from them on every launch).
 * On POSIX the path is returned verbatim: backslash is a legal filename character
 * there, so `/tmp/repo\name` and `/tmp/repo/name` are two different directories.
 */
export function normalizeWorkspacePath(
  path: string,
  platform: NodeJS.Platform = process.platform,
): string {
  return platform === 'win32' ? path.replace(/\\/g, '/') : path
}

/**
 * Comparison form: normalized separators and folded case on Windows
 * (case-insensitive filesystem — drive-letter/path casing routinely diverges between
 * sources, e.g. native dialog `C:\Source` vs msys `c:/source`); verbatim on POSIX.
 * Follows the existing convention of `comparableFsPath` (workspaceCommands) /
 * `comparablePath` (handlers): fold on win32 only, never on case-sensitive
 * filesystems. Use ONLY for comparisons and map/set keys — the stored/displayed form
 * stays case-preserved (`normalizeWorkspacePath`).
 */
export function comparableWorkspacePath(
  path: string,
  platform: NodeJS.Platform = process.platform,
): string {
  const normalized = normalizeWorkspacePath(path, platform)
  return platform === 'win32' ? normalized.toLowerCase() : normalized
}
