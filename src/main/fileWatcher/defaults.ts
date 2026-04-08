/**
 * Default glob patterns ignored by the file tree watcher.
 *
 * These cover common large/generated directories across popular stacks.
 * Dotfiles like `.env`, `.gitignore`, `.prettierrc` are intentionally NOT
 * ignored — developers need to see them in the sidebar.
 *
 * Users can override this list in Preferences → Dev Tools → File Watcher.
 */
export const DEFAULT_IGNORE_PATTERNS: readonly string[] = [
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
