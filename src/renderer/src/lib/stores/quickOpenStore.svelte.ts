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

// One listing per worktree at a time: terminal link detection asks on every hovered line
// while the list loads, and each request is a `git ls-files` (or a directory walk) in main.
// Request bookkeeping only; nothing renders from it, so it need not be reactive.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const inflight = new Map<string, Promise<string[]>>()

function startLoad(worktreePath: string, force: boolean): Promise<string[]> {
  if (!state[worktreePath]) {
    state[worktreePath] = { files: [], fetchedAt: 0, loading: true }
  } else {
    state[worktreePath].loading = true
  }
  const listing = force
    ? window.api.quickOpenListFiles(worktreePath, true)
    : window.api.quickOpenListFiles(worktreePath)
  // Only the newest request writes the result: a slower, superseded listing must not
  // overwrite the one a forced reload fetched after it.
  const request: Promise<string[]> = listing.then(
    (files) => {
      if (inflight.get(worktreePath) !== request) {
        return inflight.get(worktreePath) ?? getFiles(worktreePath)
      }
      inflight.delete(worktreePath)
      state[worktreePath] = { files, fetchedAt: Date.now(), loading: false }
      return files
    },
    () => {
      if (inflight.get(worktreePath) !== request) {
        return inflight.get(worktreePath) ?? getFiles(worktreePath)
      }
      inflight.delete(worktreePath)
      if (state[worktreePath]) state[worktreePath].loading = false
      return getFiles(worktreePath)
    },
  )
  inflight.set(worktreePath, request)
  return request
}

export async function ensureLoaded(worktreePath: string): Promise<string[]> {
  if (!worktreePath) return []
  const cached = state[worktreePath]
  if (cached && !cached.loading && Date.now() - cached.fetchedAt < STALE_AFTER_MS) {
    return cached.files
  }
  return inflight.get(worktreePath) ?? startLoad(worktreePath, false)
}

export async function forceReload(worktreePath: string): Promise<string[]> {
  if (!worktreePath) return []
  return startLoad(worktreePath, true)
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
