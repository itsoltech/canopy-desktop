interface LoadedState {
  files: string[]
  fetchedAt: number
  loading: boolean
}

const state: Record<string, LoadedState> = $state({})

const STALE_AFTER_MS = 60_000

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
    const files = await window.api.quickOpenListFiles(worktreePath)
    storeResult(worktreePath, files)
    return files
  } catch {
    return markFailed(worktreePath)
  }
}

// clearQuickOpenCache (worktree removed) can run while a listing is in flight: don't resurrect
// the evicted entry, and don't dereference it on failure.
function storeResult(worktreePath: string, files: string[]): void {
  if (state[worktreePath]) state[worktreePath] = { files, fetchedAt: Date.now(), loading: false }
}

function markFailed(worktreePath: string): string[] {
  const entry = state[worktreePath]
  if (!entry) return []
  entry.loading = false
  return entry.files
}

export async function forceReload(worktreePath: string): Promise<string[]> {
  if (!worktreePath) return []
  if (!state[worktreePath]) {
    state[worktreePath] = { files: [], fetchedAt: 0, loading: true }
  } else {
    state[worktreePath].loading = true
  }
  try {
    const files = await window.api.quickOpenListFiles(worktreePath, true)
    storeResult(worktreePath, files)
    return files
  } catch {
    return markFailed(worktreePath)
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
