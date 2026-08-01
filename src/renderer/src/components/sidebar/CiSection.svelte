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
    ChevronRight,
  } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { showPreferences, showProjectCi, showCiRunJob } from '../../lib/stores/dialogs.svelte'
  import { getCiRepoConfig, loadCiRepoConfig } from '../../lib/stores/ci.svelte'

  // CI/CD section: per-repo TeamCity — configuration entry, running any job on any
  // branch, and the server's current activity. Mirrors the Project management
  // section's architecture (config in the repo, credentials personal). Dialogs are
  // NOT rendered here: the sidebar's backdrop-filter would pin position:fixed
  // overlays to its column, so they open via dialogState from MainLayout.

  interface ActivityBuild {
    id: number
    number: string | undefined
    state: 'running' | 'queued' | 'finished'
    status: string | undefined
    percentageComplete: number | undefined
    webUrl: string
    branchName: string | undefined
    buildTypeId: string
    buildTypeName: string
  }

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

  // --- Server activity: one summary row, details (running/queued/history) on expand ---

  let activity = $state<{
    running: ActivityBuild[]
    queued: ActivityBuild[]
    recent: ActivityBuild[]
  } | null>(null)
  let activityError = $state('')
  let activityLoaded = $state(false)
  let activityExpanded = $state(false)
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
    if (activity.running.length > 0) parts.push(`${activity.running.length} running`)
    if (activity.queued.length > 0) parts.push(`${activity.queued.length} queued`)
    return parts.join(' · ') || 'Idle'
  })

  function openRunJob(): void {
    if (repoRoot) showCiRunJob(repoRoot)
  }

  function openBuild(webUrl: string): void {
    if (webUrl) window.api.openExternal(webUrl)
  }
</script>

{#snippet activityRow(build: ActivityBuild)}
  <button
    class="group flex items-center gap-2 w-full h-7 pl-5 pr-2 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:cursor-default"
    disabled={!build.webUrl}
    onclick={() => openBuild(build.webUrl)}
    title={`${build.buildTypeName}${build.number ? ` #${build.number}` : ''}${build.branchName ? ` — ${build.branchName}` : ''}${build.webUrl ? '\nOpen in TeamCity' : ''}`}
  >
    <span class="flex-1 min-w-0 truncate">{build.buildTypeName}</span>
    {#if build.branchName}
      <span class="font-mono text-2xs text-text-faint truncate max-w-24">{build.branchName}</span>
    {/if}
    {#if build.state === 'running'}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-accent-bg text-accent-text"
        >{build.percentageComplete != null ? `${build.percentageComplete}%` : 'Running'}</span
      >
    {:else if build.state === 'queued'}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-active text-text-muted"
        >Queued</span
      >
    {:else if build.status === 'SUCCESS'}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-success-bg text-success-text"
        >Success</span
      >
    {:else if build.status === 'FAILURE'}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-danger-bg text-danger-text"
        >Failed</span
      >
    {:else}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-active text-text-muted"
        >{build.status ?? 'Unknown'}</span
      >
    {/if}
  </button>
{/snippet}

<CollapsibleSection title="CI/CD" sectionKey="cicd" borderTop>
  {#snippet headerExtra()}
    {#if config}
      <button
        class="flex items-center justify-center size-5 rounded-md border-0 bg-transparent text-text-faint cursor-pointer opacity-60 hover:opacity-100 hover:bg-hover hover:text-text-secondary"
        onclick={showProjectCi}
        aria-label="Configure CI/CD"
        title="Configure CI/CD — server and available build configurations"
      >
        <Settings size={12} />
      </button>
    {/if}
  {/snippet}

  {#if !repoRoot}
    <div class="px-3 py-1 text-sm text-text-faint">Open a repository first.</div>
  {:else if !cfgState.loaded}
    <div class="flex items-center gap-2.5 h-7 px-3 text-text-faint">
      <LoaderCircle size={13} class="animate-spin flex-shrink-0" />
      <span class="text-sm">Loading…</span>
    </div>
  {:else if !config}
    <div class="px-3 py-2">
      <button
        class="flex items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
        onclick={showProjectCi}
      >
        <Plus size={14} />
        Configure TeamCity
      </button>
    </div>
  {:else}
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

        <!-- One summary row for the server's activity; details expand below. -->
        <button
          class="group flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
          onclick={() => (activityExpanded = !activityExpanded)}
          aria-expanded={activityExpanded}
          title={activityError ||
            `What is running and queued on ${serverHost}, plus recent history — click to ${activityExpanded ? 'collapse' : 'expand'}`}
        >
          <span
            class="flex items-center text-text-faint group-hover:text-text-muted transition-transform duration-base ease-std flex-shrink-0"
            class:rotate-90={activityExpanded}
          >
            <ChevronRight size={12} />
          </span>
          <span class="flex-1">Activity</span>
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

        {#if activityExpanded}
          {#if activityError}
            <div
              class="flex items-center gap-2.5 w-full min-h-7 pl-5 pr-3 py-1 text-sm text-text-faint"
              title={activityError}
            >
              <Hammer size={13} class="flex-shrink-0" />
              <span class="flex-1 truncate">{activityError}</span>
            </div>
          {:else if activity}
            {#if activeCount === 0}
              <div class="pl-5 pr-3 h-7 flex items-center text-sm text-text-faint">
                Nothing running on {serverHost}
              </div>
            {:else}
              {#each [...activity.running, ...activity.queued] as build (build.state + build.id)}
                {@render activityRow(build)}
              {/each}
            {/if}
            {#if activity.recent.length > 0}
              <span
                class="pl-5 pr-3 pt-1.5 pb-0.5 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
                >Recent</span
              >
              {#each activity.recent as build (build.id)}
                {@render activityRow(build)}
              {/each}
            {/if}
          {/if}
        {/if}
      {/if}
    </div>
  {/if}
</CollapsibleSection>
