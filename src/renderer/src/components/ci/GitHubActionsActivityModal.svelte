<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { ExternalLink, LoaderCircle, RefreshCw, X } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { getCiActivityTick } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { formatDuration, formatWhen } from '../../lib/ci/format'
  import type { CiRun, CiRunActivity } from '../../lib/ci/types'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'

  let { repoRoot }: { repoRoot: string } = $props()
  let activity = $state<CiRunActivity | null>(null)
  let loaded = $state(false)
  let refreshing = $state(false)
  let error = $state('')
  let now = $state(Date.now())
  let dialogEl: HTMLElement | undefined = $state()
  let sequence = 0

  async function refresh(): Promise<void> {
    const current = ++sequence
    refreshing = true
    try {
      const result = await window.api.ciRunActivity(repoRoot)
      if (current !== sequence) return
      activity = result
      now = Date.now()
      error = ''
    } catch (cause) {
      if (current !== sequence) return
      error = cause instanceof Error ? cause.message : 'Could not load GitHub Actions history'
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

  function runState(run: CiRun): { label: string; cls: string } {
    if (run.state === 'waiting') return { label: 'Waiting', cls: 'text-warning-text bg-warning-bg' }
    if (run.state === 'running') return { label: 'Running', cls: 'text-accent-text bg-accent-bg' }
    if (run.state === 'queued') return { label: 'Queued', cls: 'text-text-muted bg-bg-input' }
    if (run.conclusion === 'success') return { label: 'Success', cls: 'text-success bg-success-bg' }
    if (run.conclusion === 'failure')
      return { label: 'Failed', cls: 'text-danger-text bg-danger-bg' }
    if (run.conclusion === 'cancelled')
      return { label: 'Cancelled', cls: 'text-text-muted bg-bg-input' }
    if (run.conclusion === 'neutral')
      return { label: 'Neutral', cls: 'text-text-muted bg-bg-input' }
    return { label: 'Unknown', cls: 'text-text-muted bg-bg-input' }
  }
</script>

{#snippet runRow(run: CiRun)}
  {@const chip = runState(run)}
  <button
    type="button"
    class="group w-full min-h-10 px-3 py-1.5 rounded-md border-0 bg-transparent text-left text-text hover:bg-hover flex items-center gap-2.5 disabled:cursor-default"
    disabled={!run.webUrl}
    onclick={() => run.webUrl && window.api.openExternal(run.webUrl)}
    title={run.webUrl ? 'Open in GitHub Actions' : undefined}
  >
    <span class="flex-1 min-w-0 flex flex-col gap-0.5">
      <span class="flex items-baseline gap-2 min-w-0">
        <span class="truncate text-sm">{run.jobLabel}</span>
        {#if run.number}<span class="font-mono text-2xs text-text-faint">#{run.number}</span>{/if}
      </span>
      {#if run.statusText}
        <span
          class="truncate text-2xs {run.conclusion === 'success'
            ? 'text-success'
            : run.conclusion === 'failure'
              ? 'text-danger-text'
              : 'text-text-muted'}"
          title={run.statusText}>{run.statusText}</span
        >
      {/if}
      <span class="flex items-baseline gap-2 min-w-0">
        {#if run.ref}<span class="truncate font-mono text-2xs text-text-muted">{run.ref.name}</span
          >{/if}
        <span class="ml-auto shrink-0 text-2xs text-text-faint">{runMeta(run)}</span>
      </span>
    </span>
    <span class="shrink-0 px-1.5 py-px rounded-md text-2xs {chip.cls}">{chip.label}</span>
    <ExternalLink size={11} class="shrink-0 opacity-50" />
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
      <div class="flex items-center gap-1">
        <button
          type="button"
          class="size-7 rounded-md border-0 bg-transparent text-text-muted hover:bg-hover"
          onclick={() => !refreshing && void refresh()}
          aria-label="Refresh"
          disabled={refreshing}
        >
          <RefreshCw
            size={14}
            class={refreshing ? 'animate-spin motion-reduce:animate-none' : ''}
          />
        </button>
        <button
          type="button"
          class="size-7 rounded-md border-0 bg-transparent text-text-muted hover:bg-hover"
          onclick={closeDialog}
          aria-label="Close"><X size={16} /></button
        >
      </div>
    </header>
    <div class="flex-1 overflow-y-auto p-3 flex flex-col gap-3">
      {#if !loaded}
        <div class="flex items-center gap-2 p-3 text-sm text-text-muted" role="status">
          <LoaderCircle size={14} class="animate-spin-slow motion-reduce:animate-none" /> Loading history…
        </div>
      {:else if error && !activity}
        <p class="m-0 p-3 text-sm text-danger-text" role="alert">{error}</p>
      {:else if activity}
        {#if error}
          <div class="p-2 rounded-md bg-warning-bg text-xs text-warning-text" role="status">
            Could not refresh; showing the last loaded history. {error}
          </div>
        {/if}
        {#if activity.partialErrors?.length}
          <div class="p-2 rounded-md bg-warning-bg text-xs text-warning-text" role="status">
            Partial history: {activity.partialErrors.join(' · ')}
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
              No runs for the configured workflows.
            </p>{/if}
        </section>
      {/if}
    </div>
  </div>
</div>
