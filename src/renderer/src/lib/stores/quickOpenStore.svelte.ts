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

// In-flight loads per worktree. Callers that arrive while a listing is running (the
// terminal link provider on every hovered line, each LinkifiedText) share it instead of
// each starting another full repo listing. Plain Map: bookkeeping only, never rendered.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const inFlight = new Map<string, Promise<string[]>>()

export async function ensureLoaded(worktreePath: string): Promise<string[]> {
  if (!worktreePath) return []
  const cached = state[worktreePath]
  if (cached && !cached.loading && Date.now() - cached.fetchedAt < STALE_AFTER_MS) {
    return cached.files
  }
  const pending = inFlight.get(worktreePath)
  if (pending) return pending
  if (!state[worktreePath]) {
    state[worktreePath] = { files: [], fetchedAt: 0, loading: true }
  } else {
    state[worktreePath].loading = true
  }
  const load = (async () => {
    try {
      const files = await window.api.quickOpenListFiles(worktreePath)
      state[worktreePath] = { files, fetchedAt: Date.now(), loading: false }
      return files
    } catch {
      // The entry is gone if the cache was cleared (worktree detached) mid-load.
      const entry = state[worktreePath]
      if (!entry) return []
      entry.loading = false
      return entry.files
    } finally {
      inFlight.delete(worktreePath)
    }
  })()
  inFlight.set(worktreePath, load)
  return load
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
