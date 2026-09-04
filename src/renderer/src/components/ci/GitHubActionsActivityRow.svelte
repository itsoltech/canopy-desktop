<script lang="ts">
  import { ExternalLink } from '@lucide/svelte'
  import { formatDuration, formatWhen } from '../../lib/ci/format'
  import { ciRunChip, ciRunStatusTextClass } from '../../lib/ci/status'
  import { formatDateTime } from '../../lib/formatDate'
  import type { CiRun } from '../../lib/ci/types'

  let { run, now, class: className = '' }: { run: CiRun; now: number; class?: string } = $props()

  let chip = $derived(ciRunChip({ run }))
  let meta = $derived(runMeta(run, now))
  let stamp = $derived(run.startedAt ?? run.queuedAt ?? run.finishedAt)

  function runMeta(value: CiRun, timestamp: number): string {
    const parts: string[] = []
    const when = value.startedAt ?? value.queuedAt ?? value.finishedAt
    if (when != null) parts.push(formatWhen(when, timestamp))
    if (value.startedAt != null) {
      const end = value.finishedAt ?? timestamp
      parts.push(`${formatDuration(end - value.startedAt)}${value.finishedAt ? '' : ' elapsed'}`)
    }
    return parts.join(' · ')
  }
</script>

<!-- Timestamp on the top line and chip on the second, both shrink-0 at the row's right edge. -->
<button
  type="button"
  class={`group w-full min-h-10 px-3 py-1.5 rounded-md border-0 bg-transparent text-left text-text flex flex-col gap-0.5 cursor-pointer hover:bg-hover aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:opacity-60 ${className}`}
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
      <!-- Hover swaps the timestamp for the link icon. Focus keeps the timestamp visible because
           Chromium does not render title tooltips on :focus; the run number is the affordance. -->
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
      <span class="flex-1 min-w-0 truncate font-mono text-2xs text-text-muted" title={run.ref.name}
        >{run.ref.name}</span
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
