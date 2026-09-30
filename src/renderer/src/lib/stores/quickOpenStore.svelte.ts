interface LoadedState {
  files: string[]
  fetchedAt: number
  loading: boolean
}

// Raw rather than deep state: file lists can hold 100k+ paths, and a deep proxy allocates a signal
// per index on first read (fuzzy search, the terminal's known-path Set). Entries are replaced
// wholesale, never mutated, so reassigning `state` is what notifies readers.
let state: Record<string, LoadedState> = $state.raw({})

const STALE_AFTER_MS = 60_000

function setEntry(worktreePath: string, entry: LoadedState): void {
  state = { ...state, [worktreePath]: entry }
}

function markLoading(worktreePath: string): void {
  const current = state[worktreePath]
  setEntry(
    worktreePath,
    current ? { ...current, loading: true } : { files: [], fetchedAt: 0, loading: true },
  )
}

function markLoadFailed(worktreePath: string): string[] {
  const current = state[worktreePath]
  if (!current) return []
  setEntry(worktreePath, { ...current, loading: false })
  return current.files
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
  markLoading(worktreePath)
  try {
    const files = await window.api.quickOpenListFiles(worktreePath)
    setEntry(worktreePath, { files, fetchedAt: Date.now(), loading: false })
    return files
  } catch {
    return markLoadFailed(worktreePath)
  }
}

export async function forceReload(worktreePath: string): Promise<string[]> {
  if (!worktreePath) return []
  markLoading(worktreePath)
  try {
    const files = await window.api.quickOpenListFiles(worktreePath, true)
    setEntry(worktreePath, { files, fetchedAt: Date.now(), loading: false })
    return files
  } catch {
    return markLoadFailed(worktreePath)
  }
}

export function clearQuickOpenCache(worktreePath: string): void {
  if (!(worktreePath in state)) return
  const next = { ...state }
  delete next[worktreePath]
  state = next
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
