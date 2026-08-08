<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { ExternalLink, LoaderCircle, RefreshCw, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { getCiActivityTick } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { formatDuration, formatWhen } from '../../lib/ci/format'
  import { formatDateTime } from '../../lib/formatDate'
  import { ciChip, ciStatusTextClass } from '../../lib/ci/status'
  import { ipcErrorMessage } from '../../lib/ci/errors'
  import type { CiActivity, CiActivityBuild } from '../../lib/ci/types'

  // Repository activity: running, queued and recent builds whose configurations
  // are selected in this repo's CI config. Refreshes while open.

  let { repoRoot, initialBranch }: { repoRoot: string; initialBranch?: string } = $props()

  let activity = $state<CiActivity | null>(null)
  let error = $state('')
  let loaded = $state(false)
  let refreshing = $state(false)
  let now = $state(Date.now())
  let dialogEl = $state<HTMLElement>()
  let seq = 0

  /** undefined = every branch. Applied by TeamCity, not to the response — see `refresh`. */
  // svelte-ignore state_referenced_locally
  // Capturing the initial value is the point: the opener only SEEDS the filter, which
  // the user then owns. MainLayout keys this dialog by identity, so reopening it
  // remounts with a fresh seed rather than needing this to stay reactive.
  let branchFilter = $state(initialBranch)
  // Seeded from the opener because a filtered response only ever contains the branch
  // it was filtered to: without the seed, a branch with no builds at all would have no
  // option to select. Switching to "All branches" is what discovers the rest.
  // svelte-ignore state_referenced_locally
  let knownBranches = $state<string[]>(initialBranch ? [initialBranch] : [])
  let branchOptions = $derived(
    branchFilter && !knownBranches.includes(branchFilter)
      ? [...knownBranches, branchFilter].sort()
      : knownBranches,
  )

  async function refresh(): Promise<void> {
    const mySeq = ++seq
    refreshing = true
    try {
      // The branch goes into the QUERY: TeamCity applies `count:10` before the response
      // exists, so filtering here instead would blank any branch whose builds are older
      // than the ten newest in the repository.
      const result = await window.api.ciActivity(repoRoot, branchFilter)
      if (mySeq !== seq) return
      activity = result
      knownBranches = [
        ...new Set([
          ...knownBranches,
          ...[...result.running, ...result.queued, ...result.recent].flatMap((build) =>
            build.branchName ? [build.branchName] : [],
          ),
        ]),
      ].sort()
      now = Date.now()
      error = ''
    } catch (e) {
      if (mySeq !== seq) return
      error = ipcErrorMessage(e, 'Failed to load activity')
    } finally {
      if (mySeq === seq) {
        loaded = true
        refreshing = false
      }
    }
  }

  onMount(() => {
    dialogEl?.focus()
  })

  $effect(() => {
    // A trigger elsewhere in the app bumps the tick → refresh right away.
    void getCiActivityTick()
    // Tracked so picking another branch re-queries instead of re-rendering rows that
    // were fetched under the old filter, and restarts the poll on the new selection.
    void branchFilter
    untrack(() => void refresh())
    const timer = setInterval(() => void refresh(), 10_000)
    return () => clearInterval(timer)
  })

  /**
   * The instant `buildMeta` renders as relative time. Kept next to it so the row's
   * absolute-time tooltip cannot end up describing a different moment than the text
   * it annotates — the two disagree for a build that was queued but never started.
   */
  function buildStamp(build: CiActivityBuild): number | undefined {
    if (build.state === 'finished') return build.startedAt ?? build.finishedAt
    if (build.state === 'running') return build.startedAt
    return build.queuedAt
  }

  function buildMeta(build: CiActivityBuild): string {
    if (build.state === 'finished') {
      const when = build.startedAt ?? build.finishedAt
      const parts: string[] = []
      if (when != null) parts.push(formatWhen(when, now))
      if (build.startedAt != null && build.finishedAt != null) {
        parts.push(formatDuration(build.finishedAt - build.startedAt))
      }
      return parts.join(' · ')
    }
    if (build.state === 'running') {
      const parts: string[] = []
      if (build.startedAt != null) {
        parts.push(formatWhen(build.startedAt, now))
        parts.push(`${formatDuration(now - build.startedAt)} elapsed`)
      }
      return parts.join(' · ')
    }
    return build.queuedAt != null ? `queued ${formatWhen(build.queuedAt, now)}` : ''
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      closeDialog()
      return
    }
    if (e.key === 'Tab' && dialogEl) cycleFocus(dialogEl, e)
  }

  function openBuild(webUrl: string): void {
    if (webUrl) window.api.openExternal(webUrl)
  }
</script>

{#snippet buildRow(build: CiActivityBuild)}
  {@const meta = buildMeta(build)}
  {@const chip = ciChip({ build })}
  {@const stamp = buildStamp(build)}
  <!-- Timestamp on the top line and chip on the second, both shrink-0 at the row's
       right edge. They used to share one line, where the timestamp was only ml-auto
       INSIDE the text column — so its position tracked the chip's variable width
       ("Running 27%" vs "Success") and jittered on every poll as the percentage ticked. -->
  <button
    type="button"
    class="group flex shrink-0 flex-col gap-0.5 w-full min-h-8 px-3 py-1 border-0 bg-transparent text-text text-sm font-inherit text-left rounded-md transition-colors duration-fast cursor-pointer hover:bg-hover aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:opacity-60"
    aria-disabled={!build.webUrl}
    onclick={() => openBuild(build.webUrl)}
    title={build.webUrl
      ? `Open ${build.buildTypeName}${build.number ? ` #${build.number}` : ''} in TeamCity`
      : `${build.buildTypeName} cannot be opened in TeamCity`}
  >
    <span class="flex items-center gap-2 w-full min-w-0">
      <span class="flex-1 min-w-0 truncate">{build.buildTypeName}</span>
      {#if build.number}
        <span
          class="shrink-0 font-mono text-2xs text-text-faint underline-offset-2 group-hover:text-accent-text group-focus-within:text-accent-text group-hover:underline group-focus-within:underline"
          >#{build.number}</span
        >
      {/if}
      {#if meta}
        <!-- Grid overlap keeps the top line stable: HOVER swaps the timestamp for the
             link icon. Deliberately not focus — that is where a keyboard user already
             is, and Chromium never renders `title` on :focus, so the time would be gone
             with nothing to recover it. The build number carries their affordance. -->
        <span class="grid shrink-0 items-center justify-items-end">
          <span
            class="col-start-1 row-start-1 text-2xs text-text-faint whitespace-nowrap transition-opacity duration-fast {build.webUrl
              ? 'group-hover:opacity-0'
              : ''}"
            title={stamp != null ? formatDateTime(stamp) : undefined}>{meta}</span
          >
          {#if build.webUrl}
            <span
              class="col-start-1 row-start-1 flex items-center justify-end text-text-muted opacity-0 pointer-events-none transition-opacity duration-fast group-hover:opacity-100"
              aria-hidden="true"
            >
              <ExternalLink size={11} />
            </span>
          {/if}
        </span>
      {:else if build.webUrl}
        <!-- Nothing to displace here, so focus keeps its reveal. -->
        <span
          class="shrink-0 flex items-center text-text-muted opacity-0 transition-opacity duration-fast group-hover:opacity-100 group-focus-within:opacity-100"
          aria-hidden="true"
        >
          <ExternalLink size={11} />
        </span>
      {/if}
    </span>
    <span class="flex items-center gap-2 w-full min-w-0">
      {#if build.branchName}
        <span
          class="flex-1 min-w-0 truncate font-mono text-2xs text-text-muted"
          title={build.branchName}>{build.branchName}</span
        >
      {/if}
      <span class="ml-auto shrink-0 px-1.5 py-px rounded-md text-2xs {chip.cls}">{chip.label}</span>
    </span>
    {#if build.statusText}
      <span class="w-full truncate text-2xs {ciStatusTextClass(build)}" title={build.statusText}
        >{build.statusText}</span
      >
    {/if}
  </button>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onmousedown={closeDialog}
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- Native CSS resize (bottom-right handle): needs explicit dimensions and a
       non-visible overflow; the inner list scrolls independently. -->
  <div
    bind:this={dialogEl}
    class="outline-none w-[560px] h-[520px] min-w-[420px] min-h-[300px] max-w-[95vw] max-h-[92vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    style="resize: both"
    role="dialog"
    aria-modal="true"
    aria-label="Jobs history"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <header
      class="px-5 pt-4 pb-3 border-b border-border-subtle shrink-0 flex items-center justify-between gap-3"
    >
      <h2 class="text-base font-semibold text-text m-0 leading-tight shrink-0">Jobs history</h2>
      <div class="flex items-center gap-1 min-w-0">
        <label class="sr-only" for="ci-history-branch">Filter by branch</label>
        <select
          id="ci-history-branch"
          class="min-w-0 max-w-[200px] px-2 py-1 rounded-md border border-border bg-bg-input text-xs text-text"
          value={branchFilter ?? ''}
          onchange={(event) => (branchFilter = event.currentTarget.value || undefined)}
          title="Show only builds of one branch"
        >
          <option value="">All branches</option>
          {#each branchOptions as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </select>
        <!-- aria-disabled, not disabled: `refreshing` flips on a 10 s TIMER, and a
             real disabled would blur a merely-focused user to <body>, past the
             focus trap on the descendant backdrop div. -->
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0 aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
          onclick={() => {
            // Mirrors this button's aria-disabled, which does not stop clicks.
            // Scoped to the CLICK path on purpose: the 10 s poll and the
            // trigger-driven tick must still fire while a fetch is in flight —
            // seq already makes that overlap harmless, and gating them would
            // drop the re-fetch-immediately-after-a-trigger contract.
            if (!refreshing) void refresh()
          }}
          aria-disabled={refreshing}
          aria-busy={refreshing}
          aria-label="Refresh"
          title={refreshing ? 'Refreshing…' : 'Refresh now (auto-refreshes every 10 s)'}
        >
          <RefreshCw
            size={13}
            class={refreshing ? 'animate-spin-slow motion-reduce:animate-none' : ''}
          />
        </button>
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
          onclick={closeDialog}
          aria-label="Close"
          title="Close"
        >
          <X size={16} />
        </button>
      </div>
    </header>

    <div class="flex-1 overflow-y-auto px-2 py-3 flex flex-col gap-1">
      <!-- The coarse sentence is the ANNOUNCEMENT (role=status implies aria-atomic,
           so the region is read whole — a visible copy would print it a second time
           above the warning box that already says it). The wrapper's sr-only stays
           conditional: an empty in-flow child would add a gap to this flex column. -->
      <div role="status" class:sr-only={!activity?.partialErrors?.length}>
        {#if activity?.partialErrors?.length}
          <span class="sr-only">Partial history — some jobs could not be loaded</span>
          <p
            class="mx-3 mt-0 mb-2 px-2.5 py-2 rounded-md bg-warning-bg text-xs text-warning-text break-words"
            title={activity.partialErrors.join(' · ')}
            aria-hidden="true"
          >
            Partial history: {activity.partialErrors.join(' · ')}
          </p>
        {/if}
      </div>
      {#if !loaded}
        <div class="flex items-center gap-2 px-3 py-2 text-sm text-text-faint">
          <LoaderCircle size={14} class="animate-spin-slow motion-reduce:animate-none" />
          Loading activity…
        </div>
      {:else if error && !activity}
        <div class="px-3 py-2" role="alert">
          <p class="m-0 text-xs text-text-faint break-words" title={error}>{error}</p>
        </div>
      {:else if activity}
        {#if error}<p class="px-3 py-2 m-0 text-sm text-warning-text" title={error}>
            Could not refresh; showing the last loaded history. {error}
          </p>{/if}
        <span
          class="px-3 pt-1 pb-0.5 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >Running & queued</span
        >
        {#if activity.running.length === 0 && activity.queued.length === 0}
          <p class="px-3 py-1 m-0 text-sm text-text-faint">
            Nothing is running or queued{branchFilter ? ` on ${branchFilter}` : ''}.
          </p>
        {:else}
          {#each [...activity.running, ...activity.queued] as build (build.state + build.id)}
            {@render buildRow(build)}
          {/each}
        {/if}

        <span
          class="px-3 pt-3 pb-0.5 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >Recent</span
        >
        {#if activity.recent.length === 0}
          <p class="px-3 py-1 m-0 text-sm text-text-faint">
            No finished builds{branchFilter ? ` on ${branchFilter}` : ''} yet.
          </p>
        {:else}
          {#each activity.recent as build (build.id)}
            {@render buildRow(build)}
          {/each}
        {/if}
      {/if}
    </div>
  </div>
</div>
