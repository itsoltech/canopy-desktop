<script lang="ts">
  import { untrack } from 'svelte'
  import {
    Plus,
    Settings,
    ExternalLink,
    KeyRound,
    LoaderCircle,
    Play,
    Hammer,
  } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import CiLastJobCard from '../ci/CiLastJobCard.svelte'
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
  } from '../../lib/stores/ci.svelte'
  import { anyBuildActive } from '../../lib/ci/status'
  import type { CiActivityBuild } from '../../lib/ci/types'

  // CI/CD section: per-repo TeamCity — configuration entry, running any job on any
  // branch, and the server's current activity. Mirrors the Project management
  // section's architecture (config in the repo, credentials personal). Dialogs are
  // NOT rendered here: the sidebar's backdrop-filter would pin position:fixed
  // overlays to its column, so they open via dialogState from MainLayout.

  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let cfgState = $derived(getCiRepoConfig())
  let config = $derived(cfgState.config)
  let serverHost = $derived.by(() => {
    try {
      return config ? new URL(config.baseUrl).host : ''
    } catch {
      return config?.baseUrl ?? ''
    }
  })

  $effect(() => {
    const root = repoRoot
    // Untracked: the loader reads store state it also writes — tracking it here
    // would loop the effect (see the refreshCi note in the ci store).
    if (root) untrack(() => void loadCiRepoConfig(root))
  })

  // --- Server activity: one summary row; details (running/queued/history) open in
  // their own window (CiActivityModal) — the sidebar has no room for the list ---

  let activity = $state<{
    running: CiActivityBuild[]
    queued: CiActivityBuild[]
    recent: CiActivityBuild[]
  } | null>(null)
  let activityError = $state('')
  let activityLoaded = $state(false)
  let activitySeq = 0

  async function refreshActivity(root: string): Promise<void> {
    const seq = ++activitySeq
    try {
      const result = await window.api.ciActivity(root)
      if (seq !== activitySeq) return
      activity = result
      activityError = ''
    } catch (e) {
      if (seq !== activitySeq) return
      activity = null
      activityError = e instanceof Error ? e.message : 'Failed to load activity'
    } finally {
      if (seq === activitySeq) activityLoaded = true
    }
  }

  // Effect dependencies are primitives on purpose (see GitSection) — the activity
  // OBJECT changes on every poll and would loop the effect.
  let hasConfigAndToken = $derived(config != null && cfgState.hasToken)
  let activeCount = $derived(activity ? activity.running.length + activity.queued.length : 0)

  $effect(() => {
    if (!hasConfigAndToken) return
    const root = repoRoot
    if (!root) return
    // Triggering a build bumps the tick → immediate re-fetch instead of the chip
    // sitting on "Idle" until the next poll.
    void getCiActivityTick()
    untrack(() => void refreshActivity(root))
    const interval = activeCount > 0 ? 10_000 : 30_000
    const timer = setInterval(() => void refreshActivity(root), interval)
    return () => clearInterval(timer)
  })

  // Worktree switches must not show the previous repo's activity while the new
  // fetch is in flight.
  $effect(() => {
    void repoRoot
    activity = null
    activityLoaded = false
    activityError = ''
  })

  let activitySummary = $derived.by(() => {
    if (!activity) return ''
    const parts: string[] = []
    if (activity.running.length === 1) {
      // A single running build shows its actual progress right in the chip.
      const pct = activity.running[0].percentageComplete
      parts.push(pct != null ? `${pct}%` : 'running')
    } else if (activity.running.length > 1) {
      parts.push(`${activity.running.length} running`)
    }
    if (activity.queued.length > 0) parts.push(`${activity.queued.length} queued`)
    return parts.join(' · ') || 'Idle'
  })

  // --- Last build of the CURRENT branch (highlighted card) — the newest build per
  // configured job for the active worktree's branch, via ci:status ---

  let branchState = $derived(getCiState())
  let branchRows = $derived(branchState.response?.configured ? branchState.response.rows : [])
  // ci:status reports failures as a field (never throws) — surface them, or the
  // Last-job card silently vanishes with nothing naming the reason.
  let branchError = $derived(branchState.response?.error ?? '')
  // Primitive deps for the poll effect (see the activity effect above).
  let branchBuildActive = $derived(anyBuildActive(branchRows))

  // Coarse state for the live region — no percentage, so a running build announces
  // once instead of on every 10 s poll. The chip keeps the fine-grained summary.
  let ciAnnouncement = $derived.by(() => {
    if (!hasConfigAndToken) return ''
    // Branch status first: a job whose status can't be fetched is the more
    // consequential change, and it is otherwise silent (the chip and the sr-only
    // reason are only found by navigating back into the section).
    if (branchError) return 'CI status unavailable'
    const unavailable = branchRows.filter((r) => r.error).length
    if (unavailable > 0) {
      return `CI status unavailable for ${unavailable} ${unavailable === 1 ? 'job' : 'jobs'}`
    }
    if (!activityLoaded) return ''
    if (activityError) return 'CI activity unavailable'
    const running = activity?.running.length ?? 0
    const queued = activity?.queued.length ?? 0
    if (running === 0 && queued === 0) return 'CI idle'
    return `CI: ${running} running, ${queued} queued`
  })

  $effect(() => {
    if (!hasConfigAndToken) return
    const root = repoRoot
    const branch = workspaceState.branch
    if (!root || !branch) return
    void getCiActivityTick()
    untrack(() => void refreshCi(root, branch))
    const interval = branchBuildActive ? 10_000 : 45_000
    const timer = setInterval(() => void refreshCi(root, branch), interval)
    return () => clearInterval(timer)
  })

  function openRunJob(): void {
    if (repoRoot) showCiRunJob(repoRoot)
  }

  function openActivity(): void {
    if (repoRoot) showCiActivity(repoRoot)
  }
</script>

<!-- The summary chip and the row label flip on a background poll — announce the
     change instead of mutating silently under assistive tech. -->
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

  {#if repoRoot && cfgState.loaded && config}
    <div class="flex flex-col">
      <button
        class="group flex items-center gap-2.5 w-full h-7 pl-3 pr-1 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
        onclick={() => window.api.openExternal(config!.baseUrl)}
        title="Open TeamCity in the browser"
      >
        <span class="inline-flex items-center flex-shrink-0"
          ><TrackerProviderIcon provider="teamcity" size={13} /></span
        >
        <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1" title={config.baseUrl}
          >{config.baseUrl}</span
        >
        <ExternalLink
          size={11}
          class="shrink-0 opacity-0 transition-opacity duration-fast group-hover:opacity-60"
        />
      </button>

      {#if !cfgState.hasToken}
        <div class="px-2 py-1">
          <div
            class="flex items-center gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
            title={config.baseUrl}
          >
            <KeyRound size={13} class="shrink-0 text-warning-text" />
            <span class="flex-1 min-w-0 text-xs text-text-secondary leading-snug"
              >No token for this CI server.</span
            >
            <button
              type="button"
              class="shrink-0 px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
              onclick={() => showPreferences('CI connections')}
            >
              Add credentials
            </button>
          </div>
        </div>
      {:else}
        <button
          class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
          onclick={openRunJob}
          title="Queue any configured job on any branch TeamCity knows"
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

        <!-- One summary row only — the sidebar has no room for the full list, so the
             details (running / queued / recent history) open in their own window. -->
        <button
          class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
          onclick={openActivity}
          title={activityError ||
            `What is running and queued on ${serverHost}, plus recent history — opens in a window`}
        >
          <Hammer
            size={13}
            class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
          />
          <span class="flex-1">{activeCount > 0 ? 'Running job' : 'Jobs history'}</span>
          {#if activityError}
            <span
              class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-warning-bg text-warning-text"
              >Error</span
            >
          {:else if !activityLoaded}
            <LoaderCircle
              size={12}
              class="text-text-faint animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
            />
          {:else}
            <span
              class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 {activeCount > 0
                ? 'bg-accent-bg text-accent-text'
                : 'bg-active text-text-muted'}">{activitySummary}</span
            >
          {/if}
        </button>

        {#if branchError}
          <div class="px-3 py-1 text-xs text-warning-text truncate" title={branchError}>
            Last job unavailable — {branchError}
          </div>
        {:else if branchRows.length > 0 && workspaceState.branch}
          <CiLastJobCard rows={branchRows} branch={workspaceState.branch} />
        {/if}
      {/if}
    </div>
  {:else if repoRoot && cfgState.loaded}
    <!-- Init entry, mirroring Project Management's "Configure Tracker". -->
    <div class="px-3 py-2">
      <button
        class="flex items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
        onclick={showProjectCi}
      >
        <Plus size={14} />
        Configure TeamCity
      </button>
    </div>
  {/if}
</CollapsibleSection>
