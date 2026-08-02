import { untrack } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { match } from 'ts-pattern'
import { addToast, isStickyToastVisible } from './toast.svelte'

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
  /** Set when a ci block EXISTS but cannot be used (either scope) — null config
      then ≠ "no CI". */
  error?: string
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
    const res = await window.api.ciConfig(repoRoot)
    const hasToken = res.config
      ? await window.api.keychainHasCredentials('teamcity', res.config.baseUrl)
      : false
    if (seq !== configSeq) return
    // `invalid` is set exactly when a block EXISTS but cannot be used — dropping
    // the reason here would put the "Configure TeamCity" entry in front of someone
    // who already has one.
    repoConfigState = {
      key,
      loaded: true,
      config: res.config,
      hasToken,
      error: res.invalid?.message,
    }
  } catch (e) {
    if (seq !== configSeq) return
    repoConfigState = {
      key,
      loaded: true,
      config: null,
      hasToken: false,
      error: e instanceof Error ? e.message : "Could not read this repository's CI configuration",
    }
  }
}

// --- Activity refresh signal ---

// Bumped after every successful trigger so the activity views (sidebar chip, window)
// re-fetch immediately instead of sitting on "Idle" until their next poll tick.
let activityTick = $state(0)

export function getCiActivityTick(): number {
  return activityTick
}

// --- Observation of builds triggered from Canopy → completion toast ---

const OBSERVE_INTERVAL_MS = 10_000
// A hung queue shouldn't poll forever; 2h covers any realistic build.
const OBSERVE_MAX_TICKS = 720
// ~5 min of consecutive failures — a laptop suspend/resume or a VPN reconnect
// during a long build must not kill the observation (5 ticks = 50 s would).
const OBSERVE_MAX_FAILURES = 30
const observedBuilds = new SvelteMap<number, ReturnType<typeof setInterval>>()
// Give-ups cluster: the causes (VPN drop, suspend, TeamCity restart) hit every
// observed build at once, and the toast has ONE slot — so a second give-up would
// silently erase the first. Aggregate instead, keeping the job names and numbers
// (the whole point of the message is handing the user back to TeamCity). The
// cluster is "give-ups the user has not acknowledged yet", keyed on the sticky
// toast still showing — a wall-clock window would re-count dismissed builds or
// let a late give-up erase a still-displayed one.
interface GiveUp {
  label: string
  number: string | undefined
  reason: string
}
let giveUps: GiveUp[] = []

function reportGiveUp(label: string, number: string | undefined, reason: string): void {
  if (!isStickyToastVisible()) giveUps = []
  giveUps.push({ label, number, reason })
  const named = giveUps.map((g) => `${g.label}${g.number ? ` #${g.number}` : ''}`).join(', ')
  // 'default' is deliberate: a lost watcher is a status hand-off, not a build
  // outcome — danger chrome here would misread as "the build failed". When a real
  // outcome toast folds in later, the fold path escalates the chrome itself.
  addToast(
    giveUps.length > 1
      ? `Stopped watching ${giveUps.length} builds — check TeamCity: ${named}`
      : `Stopped watching ${named} — ${reason}`,
    'default',
    { sticky: true },
  )
}

function stopObserving(buildId: number): void {
  const timer = observedBuilds.get(buildId)
  if (timer) clearInterval(timer)
  observedBuilds.delete(buildId)
}

function observeBuild(repoRoot: string, buildId: number, label: string): void {
  if (observedBuilds.has(buildId)) return
  let ticks = 0
  let failures = 0
  // TeamCity's UI shows the per-configuration build NUMBER — the raw id from the
  // trigger response appears only in URLs, so it is useless for "check TeamCity".
  let lastNumber: string | undefined
  const timer = setInterval(async () => {
    ticks += 1
    if (ticks > OBSERVE_MAX_TICKS) {
      stopObserving(buildId)
      // Giving up must be audible: this observation IS the signal the user walked
      // away expecting, and silence is indistinguishable from "still building".
      // Front-loaded verb + sticky: the toast truncates at 300 px (~40 chars) and
      // normally self-dismisses in 4 s — neither may eat the hand-off, and this
      // state has no other surface in the app.
      reportGiveUp(label, lastNumber, 'still not finished after 2 h')
      return
    }
    try {
      const build = await window.api.ciBuild(repoRoot, buildId)
      failures = 0
      lastNumber = build.number
      if (build.state === 'finished') {
        stopObserving(buildId)
        // .exhaustive() so the next widening of CiBuildResult fails to compile here
        // instead of silently degrading to the neutral toast — this is the one
        // surface that reaches a user who walked away after triggering.
        match(build.status)
          .with('SUCCESS', () => addToast(`${label} #${build.number}: build succeeded`, 'success'))
          // ERROR is TeamCity's infra/agent failure — red in its own UI, so it must
          // not read as "Canopy couldn't tell".
          .with('FAILURE', 'ERROR', () =>
            addToast(`${label} #${build.number}: build failed`, 'danger'),
          )
          .with('UNKNOWN', () =>
            addToast(`${label} #${build.number}: build finished with unknown status`),
          )
          .exhaustive()
      }
    } catch {
      // Transient API errors are tolerated — but the give-up says so (see the tick
      // cap above for why it is front-loaded and sticky). The number is missing
      // only when no poll ever succeeded; the label still names the job then.
      failures += 1
      if (failures >= OBSERVE_MAX_FAILURES) {
        stopObserving(buildId)
        reportGiveUp(label, lastNumber, 'lost contact with TeamCity')
      }
    }
  }, OBSERVE_INTERVAL_MS)
  observedBuilds.set(buildId, timer)
}

/** Returns the failure message, or `null` when the build was queued. */
export async function triggerCiBuild(
  repoRoot: string,
  buildTypeId: string,
  branch: string,
  label: string,
  properties?: Array<{ name: string; value: string }>,
): Promise<string | null> {
  try {
    const result = await window.api.ciTrigger(repoRoot, buildTypeId, branch, properties)
    // TeamCity's own answer is the ground truth — if it queued on a different branch
    // than requested (e.g. fell back to the default), the toast makes that visible.
    addToast(`${label}: build queued on ${result.branchName ?? branch}`, 'success')
    observeBuild(repoRoot, result.buildId, label)
    activityTick += 1
  } catch (e) {
    // No failure toast: the only callers are the run dialogs, whose scrim
    // (z-overlay) paints OVER the toast layer (z-banner) — the message goes back
    // to the caller so it lands in the dialog's own live region instead.
    return e instanceof Error ? e.message : 'Failed to trigger build'
  }
  // Show the queued build in the row right away instead of waiting for the next poll.
  void refreshCi(repoRoot, branch)
  return null
}
