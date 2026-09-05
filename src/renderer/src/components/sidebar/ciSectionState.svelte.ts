import { untrack } from 'svelte'
import { workspaceState } from '../../lib/stores/workspace.svelte'
import { showProjectCi, showCiRunJob, showCiActivity } from '../../lib/stores/dialogs.svelte'
import {
  getCiRepoConfig,
  loadCiRepoConfig,
  getCiActivityTick,
  getCiState,
  refreshCi,
  getCiJobsState,
  refreshCiJobs,
  getCiCredentialTick,
  ciKey,
} from '../../lib/stores/ci.svelte'
import { anyBuildActive, anyRunActive } from '../../lib/ci/status'
import { ipcErrorMessage } from '../../lib/ci/errors'
import type { CiActivity, CiCardIssue, CiRunActivity } from '../../lib/ci/types'

// Keep this factory inferred so the exported ReturnType stays aligned with its reactive getters.
// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createCiSectionState() {
  // CI/CD section: per-repo TeamCity — configuration entry, running any job on any
  // branch, and the server's current activity. Mirrors the Project management
  // section's architecture (config in the repo, credentials personal). Dialogs are
  // NOT rendered here: the sidebar's backdrop-filter would pin position:fixed
  // overlays to its column, so they open via dialogState from MainLayout.

  const repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  const cfgState = $derived(getCiRepoConfig())
  const config = $derived(cfgState.config)
  const provider = $derived(config?.provider ?? 'teamcity')
  const providerLabel = $derived(provider === 'github-actions' ? 'GitHub' : 'TeamCity')
  const runActionLabel = $derived(provider === 'github-actions' ? 'Run workflow…' : 'Run job…')
  const runActionTitle = $derived(
    provider === 'github-actions'
      ? 'Choose a configured workflow and branch to run'
      : 'Choose a configured job and branch to queue',
  )
  const configureActionTitle = $derived(
    provider === 'github-actions'
      ? 'Configure CI/CD — GitHub repository and available workflows'
      : 'Configure CI/CD — server and available build configurations',
  )
  const providerUrl = $derived(
    config?.provider === 'github-actions'
      ? `https://github.com/${config.repository}`
      : (config?.baseUrl ?? ''),
  )

  $effect(() => {
    const root = repoRoot
    // Credential writers bump this after saving or removing the exact CI binding.
    void getCiCredentialTick()
    // Untracked: the loader reads store state it also writes — tracking it here
    // would loop the effect (see the refreshCi note in the ci store).
    if (root) untrack(() => void loadCiRepoConfig(root))
  })

  // True until this worktree's OWN config has landed. The store keys its state by repo, and
  // resets `loaded` whenever the key changes — so between the click and the response the
  // section knows neither the config nor whether the token still works. Everything below
  // depends on both: Run job… would queue against a server we have not re-checked, and the
  // history entry would open a window for the previous worktree.
  const ciBusy = $derived(!cfgState.loaded || cfgState.key !== (repoRoot ?? '').replace(/\\/g, '/'))
  let ciBodyEl = $state<HTMLElement>()
  let ciFrozenHeight = $state(0)

  $effect(() => {
    if (ciBusy || !ciBodyEl) return
    const el = ciBodyEl
    // Observed while idle only, so the frozen value is always the last settled render.
    const observer = new ResizeObserver(() => {
      ciFrozenHeight = el.offsetHeight
    })
    observer.observe(el)
    ciFrozenHeight = el.offsetHeight
    return () => observer.disconnect()
  })

  const ciPlaceholderRows = $derived(Math.max(1, Math.round(ciFrozenHeight / 28)))

  // --- Server activity: polled here only for its FAILURE state. The running/queued
  // list, and its counts, live in the window (CiActivityModal) — the sidebar has no
  // room for them and now shows one element, not a summary row plus a card ---

  let activity = $state<CiActivity | CiRunActivity | null>(null)
  let activityError = $state('')
  let activityLoaded = $state(false)
  let identicalPartialCount = $state(0)
  let activitySeq = 0

  async function refreshActivity(root: string): Promise<void> {
    const seq = ++activitySeq
    try {
      const result =
        config?.provider === 'github-actions'
          ? await window.api.ciRunActivity(root)
          : await window.api.ciActivity(root)
      if (seq !== activitySeq) return
      activity = result
      activityError = ''
      const hasPartial = 'partialErrors' in result && (result.partialErrors?.length ?? 0) > 0
      if (!hasPartial) {
        identicalPartialCount = 0
      } else {
        identicalPartialCount += 1
      }
    } catch (e) {
      if (seq !== activitySeq) return
      activity = null
      activityError = ipcErrorMessage(e, 'Failed to load activity')
    } finally {
      if (seq === activitySeq) activityLoaded = true
    }
  }

  // Effect dependencies are primitives on purpose (see GitSection) — the activity
  // OBJECT changes on every poll and would loop the effect.
  const credentialApprovalRequired = $derived(
    provider === 'teamcity' && cfgState.credentialApprovalRequired,
  )
  const hasConfigAndToken = $derived(
    config != null && cfgState.hasToken && !credentialApprovalRequired,
  )
  const activeCount = $derived(activity ? activity.running.length + activity.queued.length : 0)
  const activityPartialErrors = $derived(
    activity && 'partialErrors' in activity ? (activity.partialErrors ?? []) : [],
  )
  // Keep the polling effect dependent on a primitive. Depending on the derived array would
  // invalidate the effect after every response because each activity object produces a new array.
  const activityIncomplete = $derived(activityPartialErrors.length > 0)
  const fastPartialRecovery = $derived(activityIncomplete && identicalPartialCount <= 3)
  $effect(() => {
    if (!hasConfigAndToken) return
    const root = repoRoot
    if (!root) return
    // Triggering a build bumps the tick → immediate re-fetch, so a stale failure
    // suffix clears (and the cadence speeds up) without waiting for the next poll.
    void getCiActivityTick()
    // A transient missing slice can hide a running job. Retry the same partial result quickly a
    // few times, then decay to idle cadence so permanent config drift cannot poll fast forever.
    // A rejected token will keep being rejected until the user replaces it, and repeated
    // 401s are how accounts get locked out. Slow to the idle ceiling: the poll is only
    // still running so recovery is noticed without the user having to switch worktrees.
    const interval = credentialsRejected
      ? 300_000
      : provider === 'github-actions'
        ? activeCount > 0 || fastPartialRecovery
          ? 60_000
          : 300_000
        : activeCount > 0 || fastPartialRecovery
          ? 10_000
          : 30_000
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async (): Promise<void> => {
      await refreshActivity(root)
      if (!cancelled) timer = setTimeout(() => void poll(), interval)
    }
    untrack(() => void poll())
    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
    }
  })

  // Worktree switches must not show the previous repo's activity while the new
  // fetch is in flight.
  $effect(() => {
    void repoRoot
    activity = null
    activityLoaded = false
    activityError = ''
    identicalPartialCount = 0
  })

  // Only FAILURE survives in the sidebar. The running/queued counts moved into the
  // history window along with the row that carried them, so a healthy poll leaves the
  // one CI element unadorned. Unlike the old chip this is not suppressed while a build
  // is known to be active: with no counts left to prefer, hiding the failure would
  // leave nothing at all.
  // `ci:config` returns the verdict for its exact provider binding in the same response as the
  // config, so the loading gate covers it and unrelated credentials can never hide this section.
  const credentialsRejected = $derived(cfgState.authenticationState === 'invalid')
  const rejectedSince = $derived(cfgState.authenticationCheckedAt)

  const activityIssue = $derived.by((): CiCardIssue | undefined => {
    // The banner already states this one, with the action and the timestamp.
    if (credentialsRejected) return undefined
    if (activityError)
      return { label: 'Error', detail: `CI activity unavailable: ${activityError}` }
    if (activityPartialErrors.length > 0) {
      return {
        label: 'Incomplete',
        detail: `CI activity is incomplete: ${activityPartialErrors.join(' · ')}`,
      }
    }
    return undefined
  })

  // --- Last build of the CURRENT branch (highlighted card) — the newest build per
  // configured job for the active worktree's branch, via ci:status ---

  const currentCiKey = $derived(ciKey(repoRoot ?? '', workspaceState.branch ?? ''))
  const branchState = $derived(getCiState(currentCiKey))
  const jobsState = $derived(getCiJobsState(currentCiKey))
  const branchRows = $derived(branchState.response?.configured ? branchState.response.rows : [])
  const jobRows = $derived(jobsState.rows)
  // ci:status reports failures as a field (never throws) — surface them, or the
  // Last-job card silently vanishes with nothing naming the reason.
  const branchError = $derived(
    provider === 'github-actions' ? jobsState.error : (branchState.response?.error ?? ''),
  )
  // A 401 updates the main-process registry before the failing IPC reaches this component.
  // Re-read the exact configured binding when a new failure lands; token writes use the tick.
  $effect(() => {
    const root = repoRoot
    const failure = activityError || branchError
    if (root && failure) untrack(() => void loadCiRepoConfig(root))
  })
  // Primitive deps for the poll effect (see the activity effect above).
  const branchBuildActive = $derived(
    provider === 'github-actions' ? anyRunActive(jobRows) : anyBuildActive(branchRows),
  )

  const branchLoading = $derived(
    provider === 'github-actions'
      ? // Row count cannot distinguish an initial load from a settled empty result or failure.
        // Keep subsequent polls silent while retaining the previous rows/error on screen.
        jobsState.loading && !jobsState.settled
      : branchState.loading && !branchState.response,
  )
  // The card stands in for the history entry only when it has something to render.
  // Initial/recovery loading gets a non-interactive status row; after a ci:status
  // failure or on a branch with no configured jobs, the history window stays reachable.
  const hasCardRows = $derived(
    !branchLoading &&
      !branchError &&
      (provider === 'github-actions' ? jobRows.length > 0 : branchRows.length > 0),
  )

  // Coarse state for the live region — no percentage, so a running build announces
  // once instead of on every 10 s poll. Deliberately still carries the running/queued
  // counts even though nothing on screen shows them any more: the history window does,
  // and dropping them would leave screen-reader users with less than sighted ones.
  const ciAnnouncement = $derived.by(() => {
    if (ciBusy) return 'Loading CI status'
    // An unreadable ci block has no other announcement path — polling never starts.
    if (!hasConfigAndToken) return cfgState.error ? 'CI configuration invalid' : ''
    if (branchLoading || !activityLoaded) return 'Loading jobs history'
    // Both halves in one string: they are independent (a dead build-type id says
    // nothing about the server's queue), so a persistent per-row failure must not
    // shadow activity transitions for the rest of the session. Still coarse — no
    // percentage — so identical polls produce an identical string and stay quiet.
    const parts: string[] = []
    if (branchError) {
      parts.push('CI status unavailable')
    } else {
      const unavailable =
        provider === 'github-actions'
          ? jobRows.filter((row) => row.error).length
          : branchRows.filter((row) => row.error).length
      if (unavailable > 0) {
        parts.push(`CI status unavailable for ${unavailable} ${unavailable === 1 ? 'job' : 'jobs'}`)
      }
    }
    if (activityLoaded) {
      if (activityError) {
        parts.push('CI activity unavailable')
      } else {
        const running = activity?.running.length ?? 0
        const queued = activity?.queued.length ?? 0
        if (running > 0 || queued > 0) {
          parts.push(`CI: ${running} running, ${queued} queued`)
        } else if (activityPartialErrors.length === 0) {
          parts.push('CI idle')
        }
        if (activityPartialErrors.length > 0) parts.push('CI activity incomplete')
      }
    }
    return parts.join(' · ')
  })

  $effect(() => {
    // Keep a rejected credential dormant. Loading the exact binding after a token
    // replacement flips this primitive to false and immediately restarts the poll.
    if (!hasConfigAndToken || credentialsRejected) return
    const root = repoRoot
    const branch = workspaceState.branch
    if (!root || !branch) return
    void getCiActivityTick()
    const interval =
      provider === 'github-actions'
        ? branchBuildActive
          ? 60_000
          : 300_000
        : branchBuildActive
          ? 10_000
          : 45_000
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async (): Promise<void> => {
      if (provider === 'github-actions') await refreshCiJobs(root, branch)
      else await refreshCi(root, branch)
      if (!cancelled) timer = setTimeout(() => void poll(), interval)
    }
    untrack(() => void poll())
    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
    }
  })

  function openRunJob(): void {
    if (!repoRoot) return
    // Preselect the overwhelmingly common worktree branch. Dispatch still requires the shared
    // confirmation step, and the main process independently validates the target and inputs.
    showCiRunJob(repoRoot, { branch: workspaceState.branch || undefined })
  }

  function openActivity(): void {
    if (repoRoot) showCiActivity(repoRoot)
  }

  // Separate from `openActivity` on purpose: the card is branch-scoped, so it
  // preselects the window's filter and lands on the same builds it was describing —
  // which a repository-wide list, capped at the newest few, may not even contain. The
  // fallback entry stays unfiltered, matching what it says it shows.
  function openBranchActivity(): void {
    if (repoRoot) showCiActivity(repoRoot, workspaceState.branch || undefined)
  }

  function openConfigurator(section?: 'credentials'): void {
    if (repoRoot) showProjectCi(repoRoot, section)
  }

  function openProvider(): void {
    if (providerUrl) void window.api.openExternal(providerUrl)
  }

  return {
    get repoRoot() {
      return repoRoot
    },
    get cfgState() {
      return cfgState
    },
    get config() {
      return config
    },
    get provider() {
      return provider
    },
    get providerLabel() {
      return providerLabel
    },
    get providerUrl() {
      return providerUrl
    },
    get runActionLabel() {
      return runActionLabel
    },
    get runActionTitle() {
      return runActionTitle
    },
    get configureActionTitle() {
      return configureActionTitle
    },
    get ciBusy() {
      return ciBusy
    },
    get ciBodyEl() {
      return ciBodyEl
    },
    set ciBodyEl(value: HTMLElement | undefined) {
      ciBodyEl = value
    },
    get ciFrozenHeight() {
      return ciFrozenHeight
    },
    get ciPlaceholderRows() {
      return ciPlaceholderRows
    },
    get activityLoaded() {
      return activityLoaded
    },
    get credentialApprovalRequired() {
      return credentialApprovalRequired
    },
    get credentialsRejected() {
      return credentialsRejected
    },
    get rejectedSince() {
      return rejectedSince
    },
    get activityIssue() {
      return activityIssue
    },
    get branchRows() {
      return branchRows
    },
    get jobRows() {
      return jobRows
    },
    get branchError() {
      return branchError
    },
    get branchLoading() {
      return branchLoading
    },
    get hasCardRows() {
      return hasCardRows
    },
    get branch() {
      return workspaceState.branch
    },
    get ciAnnouncement() {
      return ciAnnouncement
    },
    openConfigurator,
    openProvider,
    openRunJob,
    openActivity,
    openBranchActivity,
  }
}

export type CiSectionState = ReturnType<typeof createCiSectionState>
