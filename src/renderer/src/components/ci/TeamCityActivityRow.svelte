<script lang="ts">
  import { ExternalLink } from '@lucide/svelte'
  import { formatDuration, formatWhen } from '../../lib/ci/format'
  import { ciChip, ciStatusTextClass } from '../../lib/ci/status'
  import { formatDateTime } from '../../lib/formatDate'
  import type { CiActivityBuild } from '../../lib/ci/types'

  let {
    build,
    now,
    class: className = '',
  }: { build: CiActivityBuild; now: number; class?: string } = $props()

  let meta = $derived(buildMeta(build, now))
  let chip = $derived(ciChip({ build }))
  let stamp = $derived(buildStamp(build))

  function buildStamp(value: CiActivityBuild): number | undefined {
    if (value.state === 'finished') return value.startedAt ?? value.finishedAt
    if (value.state === 'running') return value.startedAt
    return value.queuedAt
  }

  function buildMeta(value: CiActivityBuild, timestamp: number): string {
    if (value.state === 'finished') {
      const when = value.startedAt ?? value.finishedAt
      const parts: string[] = []
      if (when != null) parts.push(formatWhen(when, timestamp))
      if (value.startedAt != null && value.finishedAt != null) {
        parts.push(formatDuration(value.finishedAt - value.startedAt))
      }
      return parts.join(' · ')
    }
    if (value.state === 'running') {
      const parts: string[] = []
      if (value.startedAt != null) {
        parts.push(formatWhen(value.startedAt, timestamp))
        parts.push(`${formatDuration(timestamp - value.startedAt)} elapsed`)
      }
      return parts.join(' · ')
    }
    return value.queuedAt != null ? `queued ${formatWhen(value.queuedAt, timestamp)}` : ''
  }
</script>

<!-- Timestamp on the top line and chip on the second, both shrink-0 at the row's right edge. -->
<button
  type="button"
  class={`group flex shrink-0 flex-col gap-0.5 w-full min-h-8 px-3 py-1 border-0 bg-transparent text-text text-sm font-inherit text-left rounded-md transition-colors duration-fast cursor-pointer hover:bg-hover aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:opacity-60 ${className}`}
  aria-disabled={!build.webUrl}
  onclick={() => build.webUrl && window.api.openExternal(build.webUrl)}
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
      <!-- Hover swaps the timestamp for the link icon. Focus keeps the timestamp visible because
           Chromium does not render title tooltips on :focus; the build number is the affordance. -->
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
