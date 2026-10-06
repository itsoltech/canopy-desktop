/**
 * Guards a repo-relative file path from the renderer before it reaches a git
 * command. `..` is rejected as a path segment, not as a substring: names such as
 * `[...slug].tsx` (framework catch-all routes) or `release..notes.md` are
 * ordinary files.
 */
export function validateFilePath(filePath: string): void {
  if (filePath.startsWith('-')) throw new Error('Invalid file path: must not start with -')
  if (filePath.startsWith('/')) throw new Error('Invalid file path: must be relative')
  if (filePath.split(/[\\/]/).includes('..')) {
    throw new Error('Invalid file path: must not contain ..')
  }
}
