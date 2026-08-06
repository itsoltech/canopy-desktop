<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { ExternalLink, LoaderCircle, RefreshCw, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { getCiActivityTick } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { formatDuration, formatWhen } from '../../lib/ci/format'
  import { formatDateTime } from '../../lib/formatDate'
  import { ciRunChip, ciRunStatusTextClass } from '../../lib/ci/status'
  import { ipcErrorMessage, isCiAuthFailure } from '../../lib/ci/errors'
  import type { CiRun, CiRunActivity } from '../../lib/ci/types'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'

  let { repoRoot, initialBranch }: { repoRoot: string; initialBranch?: string } = $props()
  let activity = $state<CiRunActivity | null>(null)
  let loaded = $state(false)
  let refreshing = $state(false)
  let error = $state('')
  let now = $state(Date.now())
  let dialogEl: HTMLElement | undefined = $state()
  let sequence = 0

  /** undefined = every branch. Applied by the GitHub query, not to the response. */
  // svelte-ignore state_referenced_locally
  // Capturing the initial value is the point: the opener only SEEDS the filter, which
  // the user then owns. MainLayout keys this dialog by identity, so reopening it
  // remounts with a fresh seed rather than needing this to stay reactive.
  let branchFilter = $state(initialBranch)
  // Seeded from the opener because a filtered response only ever contains the branch
  // it was filtered to: without the seed, a branch with no runs at all would have no
  // option to select. Switching to "All branches" is what discovers the rest.
  // svelte-ignore state_referenced_locally
  let knownBranches = $state<string[]>(initialBranch ? [initialBranch] : [])
  let branchOptions = $derived(
    branchFilter && !knownBranches.includes(branchFilter)
      ? [...knownBranches, branchFilter].sort()
      : knownBranches,
  )

  async function refresh(): Promise<void> {
    const current = ++sequence
    refreshing = true
    try {
      // The branch goes into the QUERY: `recent` is sliced to the ten newest across
      // every configured workflow before we see it, so filtering here instead would
      // blank any branch whose last run is older than that.
      const result = await window.api.ciRunActivity(repoRoot, branchFilter)
      if (current !== sequence) return
      activity = result
      knownBranches = [
        ...new Set([
          ...knownBranches,
          ...[...result.running, ...result.queued, ...result.recent].flatMap((run) =>
            run.ref?.name ? [run.ref.name] : [],
          ),
        ]),
      ].sort()
      now = Date.now()
      error = ''
    } catch (cause) {
      if (current !== sequence) return
      error = ipcErrorMessage(cause, 'Could not load GitHub Actions history')
    } finally {
      if (current === sequence) {
        loaded = true
        refreshing = false
      }
    }
  }

  onMount(() => dialogEl?.focus())

  $effect(() => {
    void getCiActivityTick()
    // Tracked so picking another branch re-queries instead of re-rendering rows that
    // were fetched under the old filter, and restarts the poll on the new selection.
    void branchFilter
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async (): Promise<void> => {
      await refresh()
      if (!cancelled) timer = setTimeout(() => void poll(), 60_000)
    }
    untrack(() => void poll())
    return () => {
      cancelled = true
      sequence += 1
      if (timer) clearTimeout(timer)
    }
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      closeDialog()
    } else if (event.key === 'Tab' && dialogEl) {
      cycleFocus(dialogEl, event)
    }
  }

  function runMeta(run: CiRun): string {
    const parts: string[] = []
    const when = run.startedAt ?? run.queuedAt ?? run.finishedAt
    if (when != null) parts.push(formatWhen(when, now))
    if (run.startedAt != null) {
      const end = run.finishedAt ?? now
      parts.push(`${formatDuration(end - run.startedAt)}${run.finishedAt ? '' : ' elapsed'}`)
    }
    return parts.join(' · ')
  }
</script>

{#snippet runRow(run: CiRun)}
  {@const chip = ciRunChip({ run })}
  {@const meta = runMeta(run)}
  {@const stamp = run.startedAt ?? run.queuedAt ?? run.finishedAt}
  <!-- Timestamp on the top line and chip on the second, both shrink-0 at the row's
       right edge. They used to share one line, where the timestamp was only ml-auto
       INSIDE the text column — so its position tracked the chip's variable width
       ("Running 27%" vs "Success") and jittered on every poll as the percentage ticked. -->
  <button
    type="button"
    class="group w-full min-h-10 px-3 py-1.5 rounded-md border-0 bg-transparent text-left text-text flex flex-col gap-0.5 cursor-pointer hover:bg-hover aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:opacity-60"
    aria-disabled={!run.webUrl}
    onclick={() => run.webUrl && window.api.openExternal(run.webUrl)}
    title={run.webUrl
      ? `Open ${run.jobLabel}${run.number ? ` #${run.number}` : ''} in GitHub Actions`
      : `${run.jobLabel} cannot be opened in GitHub Actions`}
  >
    <span class="flex items-center gap-2 w-full min-w-0">
      <span class="flex-1 min-w-0 truncate text-sm">{run.jobLabel}</span>
      {#if run.number}
        <span
          class="shrink-0 font-mono text-2xs text-text-faint underline-offset-2 group-hover:text-accent-text group-focus-within:text-accent-text group-hover:underline group-focus-within:underline"
          >#{run.number}</span
        >
      {/if}
      {#if meta}
        <!-- Grid overlap keeps the top line stable: HOVER swaps the timestamp for the
             link icon. Deliberately not focus — that is where a keyboard user already
             is, and Chromium never renders `title` on :focus, so the time would be gone
             with nothing to recover it. The run number carries their affordance. -->
        <span class="grid shrink-0 items-center justify-items-end">
          <span
            class="col-start-1 row-start-1 text-2xs text-text-faint whitespace-nowrap transition-opacity duration-fast {run.webUrl
              ? 'group-hover:opacity-0'
              : ''}"
            title={stamp != null ? formatDateTime(stamp) : undefined}>{meta}</span
          >
          {#if run.webUrl}
            <span
              class="col-start-1 row-start-1 flex items-center justify-end text-text-muted opacity-0 pointer-events-none transition-opacity duration-fast group-hover:opacity-100"
              aria-hidden="true"
            >
              <ExternalLink size={11} />
            </span>
          {/if}
        </span>
      {:else if run.webUrl}
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
      {#if run.ref}
        <span
          class="flex-1 min-w-0 truncate font-mono text-2xs text-text-muted"
          title={run.ref.name}>{run.ref.name}</span
        >
      {/if}
      <span class="ml-auto shrink-0 px-1.5 py-px rounded-md text-2xs {chip.cls}">{chip.label}</span>
    </span>
    {#if run.statusText}
      <span class="w-full truncate text-2xs {ciRunStatusTextClass(run)}" title={run.statusText}
        >{run.statusText}</span
      >
    {/if}
  </button>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  onmousedown={closeDialog}
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={dialogEl}
    class="outline-none w-[560px] h-[520px] min-w-[420px] min-h-[300px] max-w-[95vw] max-h-[92vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    style="resize: both"
    role="dialog"
    aria-modal="true"
    aria-labelledby="github-history-title"
    tabindex="-1"
    onmousedown={(event) => event.stopPropagation()}
  >
    <header class="px-5 py-3 border-b border-border-subtle flex items-center justify-between">
      <h2
        id="github-history-title"
        class="m-0 text-base font-semibold text-text flex items-center gap-2"
      >
        <TrackerProviderIcon provider="github" size={17} /> Jobs history
      </h2>
      <div class="flex items-center gap-1 min-w-0">
        <label class="sr-only" for="github-history-branch">Filter by branch</label>
        <select
          id="github-history-branch"
          class="min-w-0 max-w-[200px] px-2 py-1 rounded-md border border-border bg-bg-input text-xs text-text"
          value={branchFilter ?? ''}
          onchange={(event) => (branchFilter = event.currentTarget.value || undefined)}
          title="Show only runs of one branch"
        >
          <option value="">All branches</option>
          {#each branchOptions as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </select>
        <button
          type="button"
          class="flex size-7 shrink-0 items-center justify-center rounded-md border-0 bg-transparent text-text-muted cursor-pointer hover:bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
          onclick={() => !refreshing && void refresh()}
          aria-label="Refresh"
          aria-disabled={refreshing}
          aria-busy={refreshing}
          title={refreshing ? 'Refreshing…' : 'Refresh now (auto-refreshes every 60 s)'}
        >
          <RefreshCw
            size={14}
            class={refreshing ? 'animate-spin motion-reduce:animate-none' : ''}
          />
        </button>
        <button
          type="button"
          class="flex size-7 shrink-0 items-center justify-center rounded-md border-0 bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text"
          onclick={closeDialog}
          aria-label="Close"
        >
          <X size={16} />
        </button>
      </div>
    </header>
    <div class="flex-1 overflow-y-auto p-3 flex flex-col gap-3">
      <!-- The coarse sentence is the ANNOUNCEMENT (role=status implies aria-atomic,
           so the region is read whole — a visible copy would print it a second time
           above the warning box that already says it). The wrapper's sr-only stays
           conditional: an empty in-flow child would add a gap to this flex column. -->
      <div role="status" class:sr-only={!activity?.partialErrors?.length}>
        {#if activity?.partialErrors?.length}
          <span class="sr-only">Partial history — some jobs could not be loaded</span>
          <div
            class="p-2 rounded-md bg-warning-bg text-xs text-warning-text break-words"
            title={activity.partialErrors.join(' · ')}
            aria-hidden="true"
          >
            Partial history: {activity.partialErrors.join(' · ')}
          </div>
        {/if}
      </div>
      {#if !loaded}
        <div class="flex items-center gap-2 p-3 text-sm text-text-muted" role="status">
          <LoaderCircle size={14} class="animate-spin-slow motion-reduce:animate-none" /> Loading history…
        </div>
      {:else if error && !activity}
        <!-- A rejected token is not a transient fault, so it gets a sentence naming the
             cause and the fix instead of the raw reason on its own. -->
        <div class="p-3 flex flex-col gap-1" role="alert">
          {#if isCiAuthFailure(error)}
            <p class="m-0 text-sm text-danger-text">
              GitHub rejected the stored token, so no history could be loaded.
            </p>
            <p class="m-0 text-xs text-text-muted">
              Update it from the CI/CD section's configurator, then refresh.
            </p>
          {/if}
          <p class="m-0 text-xs text-text-faint break-words" title={error}>{error}</p>
        </div>
      {:else if activity}
        {#if error}
          <div class="p-2 rounded-md bg-warning-bg text-xs text-warning-text" role="status">
            Could not refresh; showing the last loaded history. {error}
          </div>
        {/if}
        {#if activity.running.length}
          <section>
            <h3 class="m-0 px-3 py-1 text-2xs uppercase tracking-caps-tight text-text-faint">
              Running / waiting
            </h3>
            {#each activity.running as run (run.runId)}{@render runRow(run)}{/each}
          </section>
        {/if}
        {#if activity.queued.length}
          <section>
            <h3 class="m-0 px-3 py-1 text-2xs uppercase tracking-caps-tight text-text-faint">
              Queued
            </h3>
            {#each activity.queued as run (run.runId)}{@render runRow(run)}{/each}
          </section>
        {/if}
        <section>
          <h3 class="m-0 px-3 py-1 text-2xs uppercase tracking-caps-tight text-text-faint">
            Recent
          </h3>
          {#if activity.recent.length}{#each activity.recent as run (run.runId)}{@render runRow(
                run,
              )}{/each}{:else}<p class="m-0 px-3 py-4 text-sm text-text-muted">
              No runs for the configured workflows{branchFilter ? ` on ${branchFilter}` : ''}.
            </p>{/if}
        </section>
      {/if}
    </div>
  </div>
</div>
