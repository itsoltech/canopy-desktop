/**
 * Hard safety list applied natively by `FileTreeWatcher`. These directories
 * are ALWAYS excluded from native filesystem subscriptions, regardless of
 * what the user configures in Preferences. Two reasons:
 *
 *   1. Performance: watching `node_modules` or `.git` blows past inotify
 *      limits on Linux and floods the event stream during `npm install`
 *      or git operations.
 *   2. Diff pane independence: by ignoring these natively (and ONLY these),
 *      every other change reaches consumers. The sidebar then applies the
 *      user's `files.ignorePatterns` in the renderer, while diff/changes
 *      panes see everything that isn't safety-filtered.
 *
 * Dotfiles like `.env`, `.gitignore`, `.prettierrc` are intentionally NOT
 * here — developers need to see them in the sidebar.
 */
export const SAFETY_IGNORE_PATTERNS: readonly string[] = [
  'node_modules',
  '.git',
  'dist',
  'build',
  '.next',
  '.nuxt',
  '.output',
  '.svelte-kit',
  'out',
  'target',
  '.venv',
  'venv',
  '__pycache__',
  '.pytest_cache',
  '.DS_Store',
  'coverage',
  '.turbo',
  '.cache',
  '.parcel-cache',
]

/**
 * Default user-editable ignore list returned by `files:getDefaultIgnorePatterns`
 * IPC. Mirrors `SAFETY_IGNORE_PATTERNS` so the "Reset to defaults" button in
 * Preferences gives the user the same effective baseline. Kept as a separate
 * export so the two lists can diverge in the future without rippling through
 * call sites.
 */
export const DEFAULT_IGNORE_PATTERNS: readonly string[] = SAFETY_IGNORE_PATTERNS
