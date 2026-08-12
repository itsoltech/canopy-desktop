import { untrack } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { match } from 'ts-pattern'
import { addToast, isStickyToastVisible } from './toast.svelte'
import { ipcErrorMessage } from '../ci/errors'
import type { CiJobStatus, CiRepoConfigInfo, CiRun } from '../ci/types'

// CI (TeamCity) build status for the sidebar GIT section. State is scoped to one
// (repoRoot, branch) pair at a time — the section only ever shows the active worktree.

interface CiState {
  /** `repoRoot::branch` the current response belongs to. */
  key: string
  loading: boolean
  response: CiStatusResponse | null
}

const ciStates = new SvelteMap<string, CiState>()
const fetchSeqByKey = new SvelteMap<string, number>()
const CI_STATE_CACHE_MAX = 100

function prepareStateKey<T>(
  states: SvelteMap<string, T>,
  sequences: SvelteMap<string, number>,
  key: string,
): void {
  if (states.has(key)) return
  while (states.size >= CI_STATE_CACHE_MAX) {
    const oldest = states.keys().next().value
    if (oldest === undefined) break
    states.delete(oldest)
    sequences.delete(oldest)
  }
}

export function ciKey(repoRoot: string, branch: string): string {
  return `${repoRoot.replace(/\\/g, '/')}::${branch}`
}

export function getCiState(key: string): CiState {
  return ciStates.get(key) ?? { key, loading: false, response: null }
}

export async function refreshCi(repoRoot: string, branch: string): Promise<void> {
  const key = ciKey(repoRoot, branch)
  prepareStateKey(ciStates, fetchSeqByKey, key)
  const seq = (fetchSeqByKey.get(key) ?? 0) + 1
  fetchSeqByKey.set(key, seq)
  // Keep stale rows visible while re-fetching the SAME key (no flicker on poll);
  // switching to another worktree starts from a clean slate. The read is untracked:
  // this function runs from $effect bodies, and a tracked synchronous read of the
  // very state written below would loop the calling effect to death
  // (effect_update_depth_exceeded — froze the whole app).
  const previous = untrack(() => ciStates.get(key)?.response ?? null)
  ciStates.set(key, { key, loading: true, response: previous })
  try {
    const response = await window.api.ciStatus(repoRoot, branch)
    if (seq !== fetchSeqByKey.get(key)) return
    ciStates.set(key, { key, loading: false, response })
  } catch (e) {
    if (seq !== fetchSeqByKey.get(key)) return
    ciStates.set(key, {
      key,
      loading: false,
      response: {
        configured: true,
        rows: [],
        error: e instanceof Error ? e.message : 'Failed to load CI status',
      },
    })
  }
}

interface CiJobsState {
  key: string
  loading: boolean
  rows: CiJobStatus[]
  error: string
}

const jobsStates = new SvelteMap<string, CiJobsState>()
const jobsFetchSeqByKey = new SvelteMap<string, number>()

export function getCiJobsState(key: string): CiJobsState {
  return jobsStates.get(key) ?? { key, loading: false, rows: [], error: '' }
}

export async function refreshCiJobs(repoRoot: string, branch: string): Promise<void> {
  const key = ciKey(repoRoot, branch)
  prepareStateKey(jobsStates, jobsFetchSeqByKey, key)
  const sequence = (jobsFetchSeqByKey.get(key) ?? 0) + 1
  jobsFetchSeqByKey.set(key, sequence)
  const previous = untrack(() => jobsStates.get(key)?.rows ?? [])
  jobsStates.set(key, { key, loading: true, rows: previous, error: '' })
  try {
    const rows = await window.api.ciJobsStatus(repoRoot, { name: branch, kind: 'branch' })
    if (sequence !== jobsFetchSeqByKey.get(key)) return
    jobsStates.set(key, { key, loading: false, rows, error: '' })
  } catch (error) {
    if (sequence !== jobsFetchSeqByKey.get(key)) return
    jobsStates.set(key, {
      key,
      loading: false,
      rows: [],
      error: error instanceof Error ? error.message : 'Failed to load CI status',
    })
  }
}

// --- Validated per-repo CI config (shared by the CI/CD section, the GIT rows and
// the ProjectCi modal — the modal reloads it after every save) ---

interface CiRepoConfigState {
  /** Normalized repoRoot this state belongs to. */
  key: string
  loaded: boolean
  config: CiRepoConfigInfo | null
  hasToken: boolean
  authenticationState: 'valid' | 'invalid' | 'unknown'
  authenticationCheckedAt?: string
  /** Set when a ci block EXISTS but cannot be used (either scope) — null config
      then ≠ "no CI". */
  error?: string
}

let repoConfigState = $state<CiRepoConfigState>({
  key: '',
  loaded: false,
  config: null,
  hasToken: false,
  authenticationState: 'unknown',
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
    repoConfigState = {
      key,
      loaded: false,
      config: null,
      hasToken: false,
      authenticationState: 'unknown',
    }
  }
  try {
    const res = await window.api.ciConfig(repoRoot)
    if (seq !== configSeq) return
    // `invalid` is set exactly when a block EXISTS but cannot be used — dropping
    // the reason here would put the "Configure TeamCity" entry in front of someone
    // who already has one.
    repoConfigState = {
      key,
      loaded: true,
      config: res.config,
      hasToken: res.credential?.hasToken ?? false,
      authenticationState: res.credential?.authenticationState ?? 'unknown',
      authenticationCheckedAt: res.credential?.authenticationCheckedAt,
      error: res.invalid?.message,
    }
  } catch (e) {
    if (seq !== configSeq) return
    repoConfigState = {
      key,
      loaded: true,
      config: null,
      hasToken: false,
      authenticationState: 'unknown',
      error: e instanceof Error ? e.message : "Could not read this repository's CI configuration",
    }
  }
}

// --- Activity refresh signal ---

// Bumped after every successful trigger so the activity views (sidebar chip, window)
// re-fetch immediately instead of sitting on "Idle" until their next poll tick.
let activityTick = $state(0)

let credentialTick = $state(0)

export function getCiCredentialTick(): number {
  return credentialTick
}

/** Notify mounted CI surfaces after a credential is stored or removed. */
export function bumpCiCredentialTick(): void {
  credentialTick += 1
}

export function getCiActivityTick(): number {
  return activityTick
}

// --- Observation of builds triggered from Canopy → completion toast ---

const OBSERVE_INTERVAL_MS = 10_000

/**
 * The refresh fired at trigger time lands before the server knows about the run, and the
 * watcher's first tick is 10 s later — by then a GitHub queue (1-5 s typically) has already
 * become "in progress", so the Queued state never reached the card. One short follow-up
 * covers that window without adding a second poller.
 */
const QUEUED_CATCH_MS = 2_000
// Wall-clock deadlines keep their product meaning even when a request uses its full 15 s timeout.
const OBSERVE_MAX_DURATION_MS = 2 * 60 * 60 * 1_000
const OBSERVE_FAILURE_WINDOW_MS = 5 * 60 * 1_000
type CiProvider = 'teamcity' | 'github-actions'
interface BuildObservation {
  timer: ReturnType<typeof setTimeout>
  token: symbol
}
const observedBuilds = new SvelteMap<string, BuildObservation>()
// Give-ups cluster: the causes (VPN drop, suspend, TeamCity restart) hit every
// observed build at once, and the toast has ONE slot — so a second give-up would
// silently erase the first. Aggregate instead, keeping the job names and numbers
// (the whole point of the message is handing the user back to TeamCity). The
// cluster is "give-ups the user has not acknowledged yet", keyed on the sticky
// toast still showing — a wall-clock window would re-count dismissed builds or
// let a late give-up erase a still-displayed one.
interface GiveUp {
  provider: CiProvider
  label: string
  number: string | undefined
  reason: string
}
let giveUps: GiveUp[] = []

function reportGiveUp(
  provider: CiProvider,
  label: string,
  number: string | undefined,
  reason: string,
): void {
  if (!isStickyToastVisible()) giveUps = []
  giveUps.push({ provider, label, number, reason })
  const named = giveUps.map((g) => `${g.label}${g.number ? ` #${g.number}` : ''}`).join(', ')
  const providers = new Set(giveUps.map((giveUp) => giveUp.provider))
  const onlyProvider = providers.size === 1 ? giveUps[0]?.provider : undefined
  const itemNoun =
    onlyProvider === 'teamcity'
      ? 'builds'
      : onlyProvider === 'github-actions'
        ? 'workflows'
        : 'jobs'
  const providerName =
    onlyProvider === 'teamcity'
      ? 'TeamCity'
      : onlyProvider === 'github-actions'
        ? 'GitHub Actions'
        : 'the CI providers'
  // 'default' is deliberate: a lost watcher is a status hand-off, not a build
  // outcome — danger chrome here would misread as "the build failed". When a real
  // outcome toast folds in later, the fold path escalates the chrome itself.
  addToast(
    giveUps.length > 1
      ? `Stopped watching ${giveUps.length} ${itemNoun} - check ${providerName}: ${named}`
      : `Stopped watching ${named} - ${reason}`,
    'default',
    { sticky: true },
  )
}

function stopObserving(key: string, token?: symbol): void {
  const observation = observedBuilds.get(key)
  if (!observation || (token && observation.token !== token)) return
  clearTimeout(observation.timer)
  observedBuilds.delete(key)
}

function observeBuild(repoRoot: string, baseUrl: string, buildId: number, label: string): void {
  const key = `${repoRoot.replace(/\\/g, '/')}::${baseUrl}::${buildId}`
  if (observedBuilds.has(key)) return
  const token = Symbol(key)
  const startedAt = Date.now()
  let lastSuccessAt = startedAt
  // TeamCity's UI shows the per-configuration build NUMBER — the raw id from the
  // trigger response appears only in URLs, so it is useless for "check TeamCity".
  let lastNumber: string | undefined
  const isCurrent = (): boolean => observedBuilds.get(key)?.token === token
  const poll = async (): Promise<void> => {
    if (Date.now() - startedAt >= OBSERVE_MAX_DURATION_MS) {
      stopObserving(key, token)
      // Giving up must be audible: this observation IS the signal the user walked
      // away expecting, and silence is indistinguishable from "still building".
      // Front-loaded verb + sticky: the toast truncates at 300 px (~40 chars) and
      // normally self-dismisses in 4 s — neither may eat the hand-off, and this
      // state has no other surface in the app.
      reportGiveUp('teamcity', label, lastNumber, 'still not finished after 2 h')
      return
    }
    try {
      const build = await window.api.ciBuild(repoRoot, baseUrl, buildId)
      if (!isCurrent()) return
      lastSuccessAt = Date.now()
      lastNumber = build.number
      // Keep the sidebar card in step with the build being watched. The refresh fired at
      // trigger time lands BEFORE the server has the build, and the card then falls back to
      // the idle cadence (45 s) because it does not yet know anything is active — so the
      // history window showed the running build while the card still read the previous one.
      // This watcher already polls every 10 s and knows the branch; reuse it.
      if (build.branchName) void refreshCi(repoRoot, build.branchName)
      if (build.state === 'finished') {
        stopObserving(key, token)
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
      if (!isCurrent()) return
      // Transient API errors are tolerated, but the wall-clock deadline still hands
      // control back to the user. The number is missing
      // only when no poll ever succeeded; the label still names the job then.
      if (Date.now() - lastSuccessAt >= OBSERVE_FAILURE_WINDOW_MS) {
        stopObserving(key, token)
        reportGiveUp('teamcity', label, lastNumber, 'lost contact with TeamCity')
        return
      }
    }
    if (isCurrent()) {
      observedBuilds.set(key, {
        token,
        timer: setTimeout(() => void poll(), OBSERVE_INTERVAL_MS),
      })
    }
  }
  observedBuilds.set(key, {
    token,
    timer: setTimeout(() => void poll(), OBSERVE_INTERVAL_MS),
  })
}

/** Returns the failure message, or `null` when the build was queued. */
export async function triggerCiBuild(
  repoRoot: string,
  baseUrl: string,
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
    observeBuild(repoRoot, baseUrl, result.buildId, label)
    activityTick += 1
  } catch (e) {
    // No failure toast: the only callers are the run dialogs, whose scrim
    // (z-overlay) paints OVER the toast layer (z-banner) — the message goes back
    // to the caller so it lands in the dialog's own live region instead.
    return e instanceof Error ? e.message : 'Failed to trigger build'
  }
  // Show the queued build in the row right away instead of waiting for the next poll.
  void refreshCi(repoRoot, branch)
  setTimeout(() => void refreshCi(repoRoot, branch), QUEUED_CATCH_MS)
  return null
}

const observedRuns = new SvelteMap<string, ReturnType<typeof setTimeout>>()

function stopObservingRun(key: string): void {
  const timer = observedRuns.get(key)
  if (timer) clearTimeout(timer)
  observedRuns.delete(key)
}

function reportRunConclusion(run: CiRun): void {
  const number = run.number ? ` #${run.number}` : ''
  const prefix = `${run.jobLabel}${number}`
  if (run.conclusion === 'success') addToast(`${prefix}: workflow succeeded`, 'success')
  else if (run.conclusion === 'failure') addToast(`${prefix}: workflow failed`, 'danger')
  else if (run.conclusion === 'cancelled') addToast(`${prefix}: workflow cancelled`)
  else if (run.conclusion === 'neutral') addToast(`${prefix}: workflow finished (neutral)`)
  else addToast(`${prefix}: workflow finished with unknown status`)
}

function observeRun(repoRoot: string, runId: string, label: string): void {
  const key = `${repoRoot.replace(/\\/g, '/')}::${runId}`
  if (observedRuns.has(key)) return
  const startedAt = Date.now()
  let lastSuccessAt = startedAt
  const poll = async (): Promise<void> => {
    if (Date.now() - startedAt >= OBSERVE_MAX_DURATION_MS) {
      stopObservingRun(key)
      reportGiveUp(
        'github-actions',
        label,
        undefined,
        'still not finished after 2 h - check GitHub Actions',
      )
      return
    }
    try {
      const run = await window.api.ciRun(repoRoot, runId)
      if (!observedRuns.has(key)) return
      lastSuccessAt = Date.now()
      // Same reason as the TeamCity watcher above.
      if (run.ref?.name) void refreshCiJobs(repoRoot, run.ref.name)
      if (run.provider !== 'github-actions') {
        stopObservingRun(key)
        reportGiveUp(
          'github-actions',
          label,
          undefined,
          'CI provider changed - check GitHub Actions',
        )
        return
      }
      if (run.state === 'finished') {
        stopObservingRun(key)
        reportRunConclusion(run)
      }
    } catch {
      if (!observedRuns.has(key)) return
      if (Date.now() - lastSuccessAt >= OBSERVE_FAILURE_WINDOW_MS) {
        stopObservingRun(key)
        reportGiveUp('github-actions', label, undefined, 'lost contact with GitHub Actions')
        return
      }
    }
    if (observedRuns.has(key)) {
      observedRuns.set(
        key,
        setTimeout(() => void poll(), OBSERVE_INTERVAL_MS),
      )
    }
  }
  const timer = setTimeout(() => void poll(), OBSERVE_INTERVAL_MS)
  observedRuns.set(key, timer)
}

/** Deterministic cleanup for fake-timer tests; production observations stop themselves. */
export function resetCiObserversForTests(): void {
  for (const [key] of observedBuilds) stopObserving(key)
  for (const [key] of observedRuns) stopObservingRun(key)
  giveUps = []
}

export type CiTriggerIssue =
  { kind: 'cancelled' } | { kind: 'failure'; code: string; message: string; status?: number }

/** Returns `null` only when accepted; cancellation stays distinct so the form remains open. */
export async function triggerCiJob(
  repoRoot: string,
  request: {
    jobId: string
    ref: { name: string; kind: 'branch' | 'tag' }
    schemaRevision?: string
    inputs: Record<string, string | boolean>
  },
  label: string,
): Promise<CiTriggerIssue | null> {
  try {
    // Snapshot at the bridge: `ref` and `inputs` come from component $state, and a Proxy
    // cannot be structured-cloned. Plain objects pass through unchanged.
    const response = await window.api.ciTriggerJob(repoRoot, $state.snapshot(request))
    if (!response.ok) {
      if (response.error.code === 'CiDispatchCancelled') return { kind: 'cancelled' }
      return { kind: 'failure', ...response.error }
    }
    const result = response.value
    addToast(`${label}: workflow queued on ${result.ref.name}`, 'success')
    observeRun(repoRoot, result.runId, label)
    activityTick += 1
    // The tick only reaches the history window. The sidebar card is branch-scoped and fed by
    // ci:jobs, so without this it kept showing the previous run until the next poll — up to
    // 300 s away. The TeamCity path has always done the equivalent (`refreshCi`).
    void refreshCiJobs(repoRoot, result.ref.name)
    setTimeout(() => void refreshCiJobs(repoRoot, result.ref.name), QUEUED_CATCH_MS)
    return null
  } catch (error) {
    const message = ipcErrorMessage(error, 'Failed to trigger workflow')
    return { kind: 'failure', code: 'CiIpcError', message }
  }
}
