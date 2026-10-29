interface LoadedState {
  files: string[]
  fetchedAt: number
  loading: boolean
}

const state: Record<string, LoadedState> = $state({})

const STALE_AFTER_MS = 60_000

// The $state proxy allocates a reactive source per writable index it reads, and
// fuzzy search reads every path on each keystroke. Freezing the (never mutated)
// list makes those reads go straight through for repos with 100k+ files.
async function listFiles(worktreePath: string, force?: boolean): Promise<string[]> {
  const files = await window.api.quickOpenListFiles(worktreePath, force)
  Object.freeze(files)
  return files
}

export function getFiles(worktreePath: string): string[] {
  return state[worktreePath]?.files ?? []
}

export function isLoading(worktreePath: string): boolean {
  return state[worktreePath]?.loading ?? false
}

export async function ensureLoaded(worktreePath: string): Promise<string[]> {
  if (!worktreePath) return []
  const cached = state[worktreePath]
  if (cached && !cached.loading && Date.now() - cached.fetchedAt < STALE_AFTER_MS) {
    return cached.files
  }
  if (!state[worktreePath]) {
    state[worktreePath] = { files: [], fetchedAt: 0, loading: true }
  } else {
    state[worktreePath].loading = true
  }
  try {
    const files = await listFiles(worktreePath)
    state[worktreePath] = { files, fetchedAt: Date.now(), loading: false }
    return files
  } catch {
    state[worktreePath].loading = false
    return state[worktreePath].files
  }
}

export async function forceReload(worktreePath: string): Promise<string[]> {
  if (!worktreePath) return []
  if (!state[worktreePath]) {
    state[worktreePath] = { files: [], fetchedAt: 0, loading: true }
  } else {
    state[worktreePath].loading = true
  }
  try {
    const files = await listFiles(worktreePath, true)
    state[worktreePath] = { files, fetchedAt: Date.now(), loading: false }
    return files
  } catch {
    state[worktreePath].loading = false
    return state[worktreePath].files
  }
}

export function clearQuickOpenCache(worktreePath: string): void {
  delete state[worktreePath]
}

export function prefetchOnIdle(worktreePath: string): void {
  if (!worktreePath) return
  if (state[worktreePath]?.files.length) return
  const schedule =
    typeof requestIdleCallback === 'function'
      ? requestIdleCallback
      : (cb: () => void): number => setTimeout(cb, 500) as unknown as number
  schedule(() => {
    void ensureLoaded(worktreePath)
  })
}
