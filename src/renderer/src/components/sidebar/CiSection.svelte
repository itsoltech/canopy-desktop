<script lang="ts">
  import {
    Plus,
    Settings,
    ExternalLink,
    KeyRound,
    LoaderCircle,
    Play,
    ServerCog,
    Hammer,
    X,
  } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import RunBuildDialog from '../ci/RunBuildDialog.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { showPreferences, showProjectCi } from '../../lib/stores/dialogs.svelte'
  import { getCiRepoConfig, loadCiRepoConfig, triggerCiBuild } from '../../lib/stores/ci.svelte'
  import type { CiParameter } from '../../lib/ci/types'

  // CI/CD section: per-repo TeamCity — configuration entry, running any job on any
  // branch, and the server's current activity. Mirrors the Project management
  // section's architecture (config in the repo, credentials personal).

  interface ActivityBuild {
    id: number
    number: string | undefined
    state: 'running' | 'queued'
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
    if (repoRoot) void loadCiRepoConfig(repoRoot)
  })

  // --- Server activity (running + queued, whole server) ---

  let activity = $state<{ running: ActivityBuild[]; queued: ActivityBuild[] } | null>(null)
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
  let activityCount = $derived(activity ? activity.running.length + activity.queued.length : 0)

  $effect(() => {
    if (!hasConfigAndToken) return
    const root = repoRoot
    if (!root) return
    void refreshActivity(root)
    const interval = activityCount > 0 ? 10_000 : 30_000
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

  // --- Run job: any configured job on any branch TeamCity knows ---

  let runJobOpen = $state(false)
  let runJobBuildTypeId = $state('')
  let runJobBranches = $state<string[]>([])
  let runJobBranch = $state('')
  let runJobBranchesLoading = $state(false)
  let runJobError = $state('')
  let runJobStarting = $state(false)
  let runJobParams = $state<CiParameter[] | null>(null)
  let runJobSubmitting = $state(false)
  let runJobDialogEl = $state<HTMLElement>()
  let branchesSeq = 0

  let runJobLabel = $derived(
    config?.buildTypes.find((bt) => bt.id === runJobBuildTypeId)?.label ?? runJobBuildTypeId,
  )

  function openRunJob(): void {
    if (!config || config.buildTypes.length === 0) return
    runJobOpen = true
    runJobError = ''
    runJobParams = null
    runJobBuildTypeId = config.buildTypes[0].id
    void loadBranches()
  }

  function closeRunJob(): void {
    runJobOpen = false
    runJobParams = null
  }

  async function loadBranches(): Promise<void> {
    const root = repoRoot
    if (!root || !runJobBuildTypeId) return
    const seq = ++branchesSeq
    runJobBranchesLoading = true
    runJobError = ''
    try {
      const branches = await window.api.ciBranches(root, runJobBuildTypeId)
      if (seq !== branchesSeq) return
      runJobBranches = branches
      runJobBranch = branches[0] ?? ''
    } catch (e) {
      if (seq !== branchesSeq) return
      runJobBranches = []
      runJobBranch = ''
      runJobError = e instanceof Error ? e.message : 'Failed to load branches'
    } finally {
      if (seq === branchesSeq) runJobBranchesLoading = false
    }
  }

  function selectRunJob(id: string): void {
    runJobBuildTypeId = id
    void loadBranches()
  }

  /** Continue from the job+branch picker: straight trigger, or the parameters form. */
  async function startRunJob(): Promise<void> {
    const root = repoRoot
    if (!root || !runJobBuildTypeId || !runJobBranch) return
    runJobStarting = true
    runJobError = ''
    try {
      const parameters = await window.api.ciBuildParameters(root, runJobBuildTypeId)
      if (parameters.length === 0) {
        const ok = await triggerCiBuild(root, runJobBuildTypeId, runJobBranch, runJobLabel)
        if (ok) closeRunJob()
      } else {
        runJobParams = parameters
      }
    } catch (e) {
      runJobError = e instanceof Error ? e.message : 'Failed to load build parameters'
    } finally {
      runJobStarting = false
    }
  }

  async function runJobWithParameters(
    properties: Array<{ name: string; value: string }>,
  ): Promise<void> {
    const root = repoRoot
    if (!root) return
    runJobSubmitting = true
    try {
      const ok = await triggerCiBuild(
        root,
        runJobBuildTypeId,
        runJobBranch,
        runJobLabel,
        properties,
      )
      if (ok) closeRunJob()
    } finally {
      runJobSubmitting = false
    }
  }

  function handleRunJobKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      closeRunJob()
    }
  }

  $effect(() => {
    if (runJobOpen && !runJobParams) runJobDialogEl?.focus()
  })

  function openBuild(webUrl: string): void {
    if (webUrl) window.api.openExternal(webUrl)
  }
</script>

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
        <ServerCog size={13} class="text-text-faint flex-shrink-0" />
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

        <!-- Server-wide activity: everything running or queued right now. -->
        {#if activityError}
          <div
            class="flex items-center gap-2.5 w-full min-h-7 px-3 py-1 text-sm text-text-faint"
            title={activityError}
          >
            <Hammer size={13} class="flex-shrink-0" />
            <span class="flex-1 truncate">{activityError}</span>
          </div>
        {:else if !activityLoaded}
          <div class="flex items-center gap-2.5 h-7 px-3 text-sm text-text-faint">
            <LoaderCircle
              size={13}
              class="animate-spin-slow flex-shrink-0 motion-reduce:animate-none"
            />
            <span class="flex-1">Checking activity…</span>
          </div>
        {:else if activityCount === 0}
          <div class="px-3 h-7 flex items-center text-sm text-text-faint" title={config.baseUrl}>
            Nothing running on {serverHost}
          </div>
        {:else if activity}
          {#each [...activity.running, ...activity.queued] as build (build.state + build.id)}
            <button
              class="group flex items-center gap-2 w-full h-7 pl-3 pr-2 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:cursor-default"
              disabled={!build.webUrl}
              onclick={() => openBuild(build.webUrl)}
              title={`${build.buildTypeName}${build.branchName ? ` — ${build.branchName}` : ''}${build.webUrl ? '\nOpen in TeamCity' : ''}`}
            >
              <span class="flex-1 min-w-0 truncate">{build.buildTypeName}</span>
              {#if build.branchName}
                <span class="font-mono text-2xs text-text-faint truncate max-w-24"
                  >{build.branchName}</span
                >
              {/if}
              {#if build.state === 'running'}
                <span
                  class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-accent-bg text-accent-text"
                  >{build.percentageComplete != null
                    ? `${build.percentageComplete}%`
                    : 'Running'}</span
                >
              {:else}
                <span
                  class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-active text-text-muted"
                  >Queued</span
                >
              {/if}
            </button>
          {/each}
        {/if}
      {/if}
    </div>
  {/if}
</CollapsibleSection>

{#if runJobOpen && !runJobParams}
  <!-- Step 1: pick the job and the branch. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 z-[10010] flex justify-center items-center bg-scrim"
    onmousedown={closeRunJob}
    onkeydown={handleRunJobKeydown}
  >
    <div
      bind:this={runJobDialogEl}
      class="outline-none w-[420px] max-w-[92vw] flex flex-col gap-3 bg-bg-overlay border border-border rounded-xl shadow-modal p-5"
      role="dialog"
      aria-modal="true"
      aria-label="Run job"
      tabindex="-1"
      onmousedown={(e) => e.stopPropagation()}
    >
      <header class="flex items-start justify-between gap-3">
        <h3 class="text-base font-semibold text-text m-0 leading-tight">Run job</h3>
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
          onclick={closeRunJob}
          aria-label="Close"
          title="Close"
        >
          <X size={16} />
        </button>
      </header>

      <div class="flex flex-col gap-1">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">Job</span
        >
        <CustomSelect
          value={runJobBuildTypeId}
          options={(config?.buildTypes ?? []).map((bt) => ({ value: bt.id, label: bt.label }))}
          onchange={selectRunJob}
        />
      </div>

      <div class="flex flex-col gap-1">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >Branch</span
        >
        {#if runJobBranchesLoading}
          <span class="flex items-center gap-2 px-2.5 py-1.5 text-sm text-text-faint">
            <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
            Loading branches…
          </span>
        {:else if runJobBranches.length === 0}
          <span class="px-2.5 py-1.5 text-sm text-text-faint"
            >No branches known to TeamCity for this job.</span
          >
        {:else}
          <CustomSelect
            value={runJobBranch}
            options={runJobBranches.map((b) => ({ value: b, label: b }))}
            onchange={(v) => (runJobBranch = v)}
          />
        {/if}
      </div>

      <div class="min-h-4.5" aria-live="polite">
        {#if runJobError}
          <span class="text-xs text-danger-text">{runJobError}</span>
        {/if}
      </div>

      <div class="flex gap-1.5 justify-end">
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
          onclick={closeRunJob}>Cancel</button
        >
        <button
          type="button"
          class="flex items-center gap-1.5 px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
          onclick={startRunJob}
          disabled={runJobStarting || runJobBranchesLoading || !runJobBranch}
          title="Fetches the job's parameters — configurations without prompts run immediately"
        >
          {#if runJobStarting}
            <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
          {:else}
            <Play size={13} />
          {/if}
          Run
        </button>
      </div>
    </div>
  </div>
{/if}

{#if runJobOpen && runJobParams}
  <!-- Step 2: the job prompts for parameters. -->
  <RunBuildDialog
    label={runJobLabel}
    branch={runJobBranch}
    parameters={runJobParams}
    running={runJobSubmitting}
    onCancel={() => (runJobParams = null)}
    onRun={runJobWithParameters}
  />
{/if}
