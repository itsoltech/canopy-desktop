import { untrack } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { addToast } from './toast.svelte'

// CI (TeamCity) build status for the sidebar GIT section. State is scoped to one
// (repoRoot, branch) pair at a time — the section only ever shows the active worktree.

interface CiState {
  /** `repoRoot::branch` the current response belongs to. */
  key: string
  loading: boolean
  response: CiStatusResponse | null
}

let ciState = $state<CiState>({ key: '', loading: false, response: null })

// Monotonic token: a slow response for the previous worktree must not overwrite the
// state of the one the user already switched to (same pattern as the task pickers).
let fetchSeq = 0

export function ciKey(repoRoot: string, branch: string): string {
  return `${repoRoot.replace(/\\/g, '/')}::${branch}`
}

export function getCiState(): CiState {
  return ciState
}

export async function refreshCi(repoRoot: string, branch: string): Promise<void> {
  const key = ciKey(repoRoot, branch)
  const seq = ++fetchSeq
  // Keep stale rows visible while re-fetching the SAME key (no flicker on poll);
  // switching to another worktree starts from a clean slate. The read is untracked:
  // this function runs from $effect bodies, and a tracked synchronous read of the
  // very state written below would loop the calling effect to death
  // (effect_update_depth_exceeded — froze the whole app).
  const previous = untrack(() => (ciState.key === key ? ciState.response : null))
  ciState = { key, loading: true, response: previous }
  try {
    const response = await window.api.ciStatus(repoRoot, branch)
    if (seq !== fetchSeq) return
    ciState = { key, loading: false, response }
  } catch (e) {
    if (seq !== fetchSeq) return
    ciState = {
      key,
      loading: false,
      response: {
        configured: true,
        rows: [],
        error: e instanceof Error ? e.message : 'Failed to load CI status',
      },
    }
  }
}

// --- Validated per-repo CI config (shared by the CI/CD section, the GIT rows and
// the ProjectCi modal — the modal reloads it after every save) ---

interface CiRepoConfigState {
  /** Normalized repoRoot this state belongs to. */
  key: string
  loaded: boolean
  config: { baseUrl: string; buildTypes: Array<{ id: string; label: string }> } | null
  hasToken: boolean
}

let repoConfigState = $state<CiRepoConfigState>({
  key: '',
  loaded: false,
  config: null,
  hasToken: false,
})
let configSeq = 0

export function getCiRepoConfig(): CiRepoConfigState {
  return repoConfigState
}

export async function loadCiRepoConfig(repoRoot: string): Promise<void> {
  const key = repoRoot.replace(/\\/g, '/')
  const seq = ++configSeq
  // Untracked for the same reason as in refreshCi — callers are $effect bodies.
  if (untrack(() => repoConfigState.key) !== key) {
    repoConfigState = { key, loaded: false, config: null, hasToken: false }
  }
  try {
    const config = await window.api.ciConfig(repoRoot)
    const hasToken = config
      ? await window.api.keychainHasCredentials('teamcity', config.baseUrl)
      : false
    if (seq !== configSeq) return
    repoConfigState = { key, loaded: true, config, hasToken }
  } catch {
    if (seq !== configSeq) return
    repoConfigState = { key, loaded: true, config: null, hasToken: false }
  }
}

// --- Observation of builds triggered from Canopy → completion toast ---

const OBSERVE_INTERVAL_MS = 10_000
// A hung queue shouldn't poll forever; 2h covers any realistic build.
const OBSERVE_MAX_TICKS = 720
const observedBuilds = new SvelteMap<number, ReturnType<typeof setInterval>>()

function stopObserving(buildId: number): void {
  const timer = observedBuilds.get(buildId)
  if (timer) clearInterval(timer)
  observedBuilds.delete(buildId)
}

function observeBuild(repoRoot: string, buildId: number, label: string): void {
  if (observedBuilds.has(buildId)) return
  let ticks = 0
  let failures = 0
  const timer = setInterval(async () => {
    ticks += 1
    if (ticks > OBSERVE_MAX_TICKS) {
      stopObserving(buildId)
      return
    }
    try {
      const build = await window.api.ciBuild(repoRoot, buildId)
      failures = 0
      if (build.state === 'finished') {
        stopObserving(buildId)
        addToast(
          build.status === 'SUCCESS'
            ? `${label} #${build.number}: build succeeded`
            : `${label} #${build.number}: build ${build.status === 'FAILURE' ? 'failed' : 'finished with unknown status'}`,
        )
      }
    } catch {
      // Transient API errors are tolerated; give up after a few in a row.
      failures += 1
      if (failures >= 5) stopObserving(buildId)
    }
  }, OBSERVE_INTERVAL_MS)
  observedBuilds.set(buildId, timer)
}

export async function triggerCiBuild(
  repoRoot: string,
  buildTypeId: string,
  branch: string,
  label: string,
  properties?: Array<{ name: string; value: string }>,
): Promise<boolean> {
  try {
    const result = await window.api.ciTrigger(repoRoot, buildTypeId, branch, properties)
    // TeamCity's own answer is the ground truth — if it queued on a different branch
    // than requested (e.g. fell back to the default), the toast makes that visible.
    addToast(`${label}: build queued on ${result.branchName ?? branch}`)
    observeBuild(repoRoot, result.buildId, label)
  } catch (e) {
    addToast(e instanceof Error ? e.message : 'Failed to trigger build')
    return false
  }
  // Show the queued build in the row right away instead of waiting for the next poll.
  void refreshCi(repoRoot, branch)
  return true
}
