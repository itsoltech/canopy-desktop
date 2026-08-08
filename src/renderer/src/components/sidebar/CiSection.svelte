<script lang="ts">
  import { untrack } from 'svelte'
  import {
    Plus,
    Settings,
    ExternalLink,
    KeyRound,
    LoaderCircle,
    Play,
    History,
  } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import CiLastJobCard from '../ci/CiLastJobCard.svelte'
  import CiLastRunCard from '../ci/CiLastRunCard.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import {
    showPreferences,
    showProjectCi,
    showCiRunJob,
    showCiActivity,
  } from '../../lib/stores/dialogs.svelte'
  import {
    getCiRepoConfig,
    loadCiRepoConfig,
    getCiActivityTick,
    getCiState,
    refreshCi,
    getCiJobsState,
    refreshCiJobs,
  } from '../../lib/stores/ci.svelte'
  import { anyBuildActive, anyRunActive } from '../../lib/ci/status'
  import { ipcErrorMessage, isCiAuthFailure } from '../../lib/ci/errors'
  import { formatDateTime } from '../../lib/formatDate'
  import type { CiActivity, CiCardIssue, CiRunActivity } from '../../lib/ci/types'

  // CI/CD section: per-repo TeamCity — configuration entry, running any job on any
  // branch, and the server's current activity. Mirrors the Project management
  // section's architecture (config in the repo, credentials personal). Dialogs are
  // NOT rendered here: the sidebar's backdrop-filter would pin position:fixed
  // overlays to its column, so they open via dialogState from MainLayout.

  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let cfgState = $derived(getCiRepoConfig())
  let config = $derived(cfgState.config)
  let provider = $derived(config?.provider ?? 'teamcity')
  let providerLabel = $derived(provider === 'github-actions' ? 'GitHub' : 'TeamCity')
  let providerUrl = $derived(
    config?.provider === 'github-actions'
      ? `https://github.com/${config.repository}`
      : (config?.baseUrl ?? ''),
  )

  $effect(() => {
    const root = repoRoot
    // Untracked: the loader reads store state it also writes — tracking it here
    // would loop the effect (see the refreshCi note in the ci store).
    if (root) untrack(() => void loadCiRepoConfig(root))
  })

  // True until this worktree's OWN config has landed. The store keys its state by repo, and
  // resets `loaded` whenever the key changes — so between the click and the response the
  // section knows neither the config nor whether the token still works. Everything below
  // depends on both: Run job… would queue against a server we have not re-checked, and the
  // history entry would open a window for the previous worktree.
  let ciBusy = $derived(!cfgState.loaded || cfgState.key !== (repoRoot ?? '').replace(/\\/g, '/'))
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

  let ciPlaceholderRows = $derived(Math.max(1, Math.round(ciFrozenHeight / 28)))

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
  let hasConfigAndToken = $derived(config != null && cfgState.hasToken)
  let activeCount = $derived(activity ? activity.running.length + activity.queued.length : 0)
  let activityPartialErrors = $derived(
    activity && 'partialErrors' in activity ? (activity.partialErrors ?? []) : [],
  )
  // Keep the polling effect dependent on a primitive. Depending on the derived array would
  // invalidate the effect after every response because each activity object produces a new array.
  let activityIncomplete = $derived(activityPartialErrors.length > 0)
  let fastPartialRecovery = $derived(activityIncomplete && identicalPartialCount <= 3)
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
  // Token validity is answered LOCALLY, not by waiting for history. The registry persists
  // `authenticationState` from the last 401, so this is one in-process IPC (milliseconds)
  // rather than a network round-trip — which is why Run job… and the card can be withheld
  // from the first frame instead of appearing and being taken away a second later.
  let storedAuth = $state<{ state: string; checkedAt?: string } | undefined>(undefined)
  $effect(() => {
    const url = providerUrl
    // Re-read after a failure lands too, so a fresh 401 updates the "since" stamp.
    void activityError
    void branchError
    if (!url) {
      storedAuth = undefined
      return
    }
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const read = async (): Promise<void> => {
      try {
        const stored = await window.api.keychainListCredentials()
        const match = stored.find((entry) => entry.baseUrl === url)
        if (cancelled) return
        storedAuth = match
          ? { state: match.authenticationState, checkedAt: match.authenticationCheckedAt }
          : undefined
      } catch {
        if (!cancelled) storedAuth = undefined
      }
      // Saving a replacement token resets the credential to `unknown`, but nothing in this
      // section observes that: the config does not reload, the tick does not fire, and the
      // activity poll is deliberately at 300 s while a token is rejected. So while the verdict
      // says `invalid`, re-read it directly — this is an in-process IPC, not a request to the
      // server, so a short interval costs nothing and the banner clears on its own.
      if (!cancelled && storedAuth?.state === 'invalid') {
        timer = setTimeout(() => void read(), 5_000)
      }
    }
    void read()
    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
    }
  })

  // The stored verdict is what removes the flicker; the live errors keep it correct for a
  // token that only just started being rejected. History loads independently of both.
  let credentialsRejected = $derived(
    storedAuth?.state === 'invalid' ||
      isCiAuthFailure(activityError) ||
      isCiAuthFailure(branchError),
  )
  let rejectedSince = $derived(storedAuth?.checkedAt)

  let activityIssue = $derived.by((): CiCardIssue | undefined => {
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

  let branchState = $derived(getCiState())
  let jobsState = $derived(getCiJobsState())
  let branchRows = $derived(branchState.response?.configured ? branchState.response.rows : [])
  let jobRows = $derived(jobsState.rows)
  // ci:status reports failures as a field (never throws) — surface them, or the
  // Last-job card silently vanishes with nothing naming the reason.
  let branchError = $derived(
    provider === 'github-actions' ? jobsState.error : (branchState.response?.error ?? ''),
  )
  // Primitive deps for the poll effect (see the activity effect above).
  let branchBuildActive = $derived(
    provider === 'github-actions' ? anyRunActive(jobRows) : anyBuildActive(branchRows),
  )

  let branchLoading = $derived(
    provider === 'github-actions'
      ? jobsState.loading && jobRows.length === 0
      : branchState.loading && !branchState.response,
  )
  // The card stands in for the history entry only when it has something to render.
  // While loading, after a ci:status failure, or on a branch with no configured jobs
  // it renders nothing at all — and the history window has to stay reachable there.
  let hasCardRows = $derived(
    !branchLoading &&
      !branchError &&
      (provider === 'github-actions' ? jobRows.length > 0 : branchRows.length > 0),
  )

  // Coarse state for the live region — no percentage, so a running build announces
  // once instead of on every 10 s poll. Deliberately still carries the running/queued
  // counts even though nothing on screen shows them any more: the history window does,
  // and dropping them would leave screen-reader users with less than sighted ones.
  let ciAnnouncement = $derived.by(() => {
    // An unreadable ci block has no other announcement path — polling never starts.
    if (!hasConfigAndToken) return cfgState.error ? 'CI configuration invalid' : ''
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
    if (!hasConfigAndToken) return
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
</script>

<!-- The card's heading and status flip on a background poll — announce the change
     instead of mutating silently under assistive tech. -->
<span class="sr-only" aria-live="polite">{ciAnnouncement}</span>
<CollapsibleSection title="CI/CD" sectionKey="cicd" borderTop>
  {#snippet headerExtra()}
    <!-- Always available — for unconfigured worktrees this is the ONLY entry point
         (their section body stays empty on purpose). -->
    <button
      class="flex items-center justify-center size-5 rounded-md border-0 bg-transparent text-text-faint cursor-pointer opacity-60 hover:opacity-100 hover:bg-hover hover:text-text-secondary"
      onclick={showProjectCi}
      aria-label="Configure CI/CD"
      title="Configure CI/CD — server and available build configurations"
    >
      <Settings size={12} />
    </button>
  {/snippet}

  <!-- Same treatment as the tracker section: while the next worktree's CI config and token
       state are unknown, cover the body instead of tearing it down. Run job… and the history
       entry must not stay clickable on a token we cannot vouch for yet, and cfgState.loaded
       drops to false on every switch — which collapsed this whole section to nothing and
       rebuilt it, moving every section below twice. -->
  <div
    bind:this={ciBodyEl}
    class="flex flex-col {ciBusy ? 'overflow-hidden' : ''}"
    style:height={ciBusy && ciFrozenHeight > 0 ? `${ciFrozenHeight}px` : undefined}
  >
    {#if ciBusy && ciFrozenHeight > 0}
      <div class="flex flex-col" aria-hidden="true">
        {#each { length: ciPlaceholderRows }, i (i)}
          <div class="flex items-center h-7 px-3">
            <span
              class="h-2 rounded-sm bg-active animate-pulse motion-reduce:animate-none"
              style:width={`${[58, 40, 70, 48][i % 4]}%`}
            ></span>
          </div>
        {/each}
      </div>
      <span class="sr-only" role="status">Loading CI status…</span>
    {:else if repoRoot && cfgState.loaded && config}
      <div class="flex flex-col">
        <button
          class="group flex items-center gap-2.5 w-full h-7 pl-3 pr-1 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
          onclick={() => window.api.openExternal(providerUrl)}
          title={provider === 'github-actions'
            ? 'Open repository in GitHub'
            : 'Open TeamCity in the browser'}
        >
          <span class="inline-flex items-center flex-shrink-0"
            ><TrackerProviderIcon
              provider={provider === 'github-actions' ? 'github' : 'teamcity'}
              size={13}
            /></span
          >
          <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1" title={providerUrl}
            >{config.provider === 'github-actions' ? config.repository : config.baseUrl}</span
          >
          <ExternalLink
            size={11}
            class="shrink-0 opacity-0 transition-opacity duration-fast group-hover:opacity-60"
          />
        </button>

        <!-- A token that is missing and one the server rejects lead to the same dead ends:
           Run job… cannot list branches or queue anything, and the history window has
           nothing to load. So the banner REPLACES them rather than sitting above them —
           the only thing left worth offering is fixing the credential. -->
        {#if !cfgState.hasToken || credentialsRejected}
          <div class="px-2 py-1">
            <div
              class="flex items-center gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
              title={credentialsRejected ? activityError || branchError : config.baseUrl}
            >
              <KeyRound size={13} class="shrink-0 text-warning-text" />
              <span class="flex-1 min-w-0 text-xs text-text-secondary leading-snug">
                {#if credentialsRejected}
                  {providerLabel} rejected the stored token.{rejectedSince
                    ? ` Since ${formatDateTime(Date.parse(rejectedSince))}.`
                    : ''}
                {:else}
                  No token for this CI server.
                {/if}
              </span>
              <button
                type="button"
                class="shrink-0 px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
                onclick={() =>
                  provider === 'github-actions'
                    ? showProjectCi()
                    : showPreferences('CI connections')}
              >
                {credentialsRejected ? 'Update token' : 'Add credentials'}
              </button>
            </div>
          </div>
        {:else}
          <button
            class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
            onclick={openRunJob}
            title="Choose a configured job and branch to queue"
          >
            <Play
              size={13}
              class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
            />
            <span class="flex-1">Run job…</span>
          </button>

          <div
            class="h-px mx-3 my-1 bg-border-subtle"
            role="separator"
            aria-orientation="horizontal"
          ></div>

          <!-- ONE CI element, never two: the branch card when it has rows, otherwise the
             plain history entry. Both open the same window; the card also preselects
             its branch there. The entry is not redundant with the card — it is the only
             route to the window while the branch has no configured jobs, no branch is
             checked out, or ci:status is still in flight, all states in which the card
             renders nothing. Keeping them in one if/else makes showing both impossible. -->
          {#if hasCardRows && workspaceState.branch}
            {#if provider === 'github-actions'}
              <CiLastRunCard
                rows={jobRows}
                branch={workspaceState.branch}
                issue={activityIssue}
                onActivate={openBranchActivity}
              />
            {:else}
              <CiLastJobCard
                rows={branchRows}
                branch={workspaceState.branch}
                issue={activityIssue}
                onActivate={openBranchActivity}
              />
            {/if}
          {:else}
            <button
              class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
              onclick={openActivity}
              aria-haspopup="dialog"
              title={`Recent and running jobs for this repository — opens in a window${
                activityIssue ? ` (${activityIssue.detail})` : ''
              }`}
            >
              <History size={13} class="text-text-faint group-hover:text-text-secondary shrink-0" />
              <span class="flex-1">Jobs history</span>
              {#if activityIssue}
                <span
                  class="px-1.5 py-px rounded-md text-2xs shrink-0 bg-warning-bg text-warning-text"
                  title={activityIssue.detail}>{activityIssue.label}</span
                >
              {:else if !activityLoaded}
                <LoaderCircle
                  size={12}
                  class="text-text-faint animate-spin-slow shrink-0 motion-reduce:animate-none"
                />
              {/if}
            </button>

            <!-- Diagnostics stay a sub-line under the entry rather than folding into it:
               they name a reason, and a reason with no visible route to act on it is
               what this section was already pulled up on once. -->
            {#if branchLoading}
              <div class="px-3 py-1 flex items-center gap-2 text-xs text-text-faint">
                <LoaderCircle size={12} class="animate-spin-slow motion-reduce:animate-none" />
                Checking CI status…
              </div>
            {:else if branchError}
              <div class="px-3 py-1 text-xs text-warning-text truncate" title={branchError}>
                Last job unavailable — {branchError}
              </div>
            {/if}
          {/if}
        {/if}
      </div>
    {:else if repoRoot && cfgState.loaded}
      {#if cfgState.error}
        <!-- The block EXISTS but cannot be used — a "Configure TeamCity" entry here
           would send the user to set up what they already have. ciErrorMessage
           front-loads the reason for this truncated column (rendered verbatim —
           never re-parsed), and the recovery is a visible button, not just the
           header gear. -->
        <div class="px-3 py-1 flex flex-col gap-0.5 text-xs text-warning-text">
          <span class="truncate" title={cfgState.error}>{cfgState.error}</span>
          <button
            type="button"
            class="self-start text-2xs underline underline-offset-2 bg-transparent border-0 p-0 font-inherit text-warning-text cursor-pointer hover:text-text"
            onclick={showProjectCi}>Open the configurator</button
          >
        </div>
      {:else}
        <!-- Init entry, mirroring Project Management's "Configure Tracker". -->
        <div class="px-3 py-2">
          <button
            class="flex items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
            onclick={showProjectCi}
          >
            <Plus size={14} />
            Configure CI/CD
          </button>
        </div>
      {/if}
    {/if}
  </div>
</CollapsibleSection>
