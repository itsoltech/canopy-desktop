<script lang="ts">
  import { History } from '@lucide/svelte'
  import { formatDateTime } from '../../lib/formatDate'
  import type { CiCardIssue, CiLastStatusRow } from '../../lib/ci/types'

  let {
    row,
    branch,
    active,
    issue,
    onActivate,
  }: {
    /**
     * The ONE last run for this branch — the wrappers pick it, this only renders it.
     * Optional and guarded INSIDE this component on purpose: props are read lazily, so a
     * parent `{#if row}` still lets this component re-run its effects once with the row
     * already gone, and dereferencing it there throws through the whole CI section.
     */
    row: CiLastStatusRow | undefined
    branch: string
    /** That run is queued or running — flips the heading to "Running job". */
    active: boolean
    issue?: CiCardIssue
    onActivate: () => void
  } = $props()

  // One wording for both providers. "Running run" is unusable, and "Running job" paired
  // with "Last run" would swap the NOUN every time a build ends. "Job" is this app's
  // cross-provider word already — the GitHub history window's own heading says
  // "Jobs history", and the row above this card says "Run job…" for both.
  let cardTitle = $derived(active ? 'Running job' : 'Last job')
  let cardHint = $derived(
    `${cardTitle} for ${branch} — opens jobs history${issue ? ` (${issue.detail})` : ''}`,
  )
</script>

<!-- One control and one run. Every configured job's newest build used to be listed here,
     each its own button; they all lead to the same window now, and the sidebar's question
     is "what happened last on this branch", which has a single answer. -->
{#if row}
  <button
    type="button"
    class="group/card mx-2 my-1 px-2.5 py-1.5 rounded-lg border border-accent-muted flex flex-col gap-1 bg-transparent text-left font-inherit cursor-pointer hover:bg-hover"
    onclick={onActivate}
    aria-haspopup="dialog"
    title={cardHint}
  >
    <span class="flex items-center gap-2 w-full min-w-0">
      <span
        class="flex-1 min-w-0 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint truncate"
        title={row.label}>{cardTitle} · {row.label}</span
      >
      {#if issue}
        <!-- Left in the accessible tree deliberately: it is two words, and the live region
             in CiSection announces the CHANGE, not this static label. -->
        <span
          class="shrink-0 text-2xs font-semibold uppercase tracking-caps-tight text-warning-text"
          title={issue.detail}>· {issue.label}</span
        >
      {/if}
      {#if row.timestamp != null}
        <!-- Grid overlap keeps the top line stable: HOVER replaces the date with the history
             icon. Deliberately not focus — focus is where a keyboard user IS, and blanking
             the date there loses it for good (Chromium renders `title` on hover only, never
             on :focus). Their affordance is the card's own focus ring. -->
        <span class="grid shrink-0 items-center justify-items-end">
          <time
            class="col-start-1 row-start-1 text-2xs text-text-faint text-right transition-opacity duration-fast group-hover/card:opacity-0"
            datetime={new Date(row.timestamp).toISOString()}
            title={`${row.timestampLabel ?? 'Updated'} ${formatDateTime(row.timestamp)}`}
            ><span class="sr-only">{row.timestampLabel ?? 'Updated'} </span>{formatDateTime(
              row.timestamp,
            )}</time
          >
          <span
            class="col-start-1 row-start-1 flex items-center justify-end text-text-muted opacity-0 pointer-events-none transition-opacity duration-fast group-hover/card:opacity-100"
            aria-hidden="true"
          >
            <History size={11} />
          </span>
        </span>
      {:else}
        <!-- No date to displace here, so focus keeps its reveal: the asymmetry with the
             branch above is the point — that one would have to hide information to show
             this icon, this one costs nothing. -->
        <span
          class="flex items-center justify-center text-text-muted opacity-0 transition-opacity duration-fast group-hover/card:opacity-100 group-focus-within/card:opacity-100 shrink-0"
          aria-hidden="true"
        >
          <History size={11} />
        </span>
      {/if}
    </span>

    <span class="flex items-center gap-2 w-full min-w-0">
      <span class="flex-1 min-w-0 truncate font-mono text-xs text-text-muted" title={branch}
        >{branch}</span
      >
      {#if row.number}
        <span class="font-mono text-2xs text-text-secondary flex-shrink-0">#{row.number}</span>
      {/if}
      <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 {row.chip.cls}"
        >{row.chip.label}</span
      >
      {#if row.error}<span class="sr-only">{row.error}</span>{/if}
    </span>

    {#if row.statusText}
      <span
        class="w-full truncate text-2xs {row.statusTextClass ?? 'text-text-muted'}"
        title={row.statusText}>{row.statusText}</span
      >
    {:else if row.error}
      <span
        class="w-full truncate text-2xs text-warning-text opacity-0 group-hover/card:opacity-100 group-focus-within/card:opacity-100 transition-opacity duration-fast"
        title={row.error}
        aria-hidden="true">{row.error}</span
      >
    {/if}
  </button>
{/if}
