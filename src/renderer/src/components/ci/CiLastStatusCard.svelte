<script lang="ts">
  import { History } from '@lucide/svelte'
  import { formatDateTime } from '../../lib/formatDate'
  import type { CiCardIssue } from '../../lib/ci/types'

  interface LastStatusRow {
    id: string
    label: string
    number?: string
    timestamp?: number
    timestampLabel?: 'Queued' | 'Started' | 'Finished'
    statusText?: string
    statusTextClass?: string
    error?: string
    chip: { label: string; cls: string }
  }

  let {
    rows,
    branch,
    active,
    issue,
    onActivate,
  }: {
    rows: LastStatusRow[]
    branch: string
    /** Any of these rows queued or running — flips the heading to "Running job". */
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

{#snippet stamp(row: LastStatusRow)}
  {#if row.timestamp != null}
    <time
      class="text-2xs text-text-faint text-right"
      datetime={new Date(row.timestamp).toISOString()}
      title={`${row.timestampLabel ?? 'Updated'} ${formatDateTime(row.timestamp)}`}
      ><span class="sr-only">{row.timestampLabel ?? 'Updated'} </span>{formatDateTime(
        row.timestamp,
      )}</time
    >
  {/if}
{/snippet}

{#snippet issueSuffix()}
  {#if issue}
    <!-- Left in the accessible tree deliberately: it is two words, and the live region
         in CiSection announces the CHANGE, not this static label. -->
    <span
      class="shrink-0 text-2xs font-semibold uppercase tracking-caps-tight text-warning-text"
      title={issue.detail}>· {issue.label}</span
    >
  {/if}
{/snippet}

{#snippet statusLine(row: LastStatusRow)}
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
{/snippet}

{#snippet details(row: LastStatusRow)}
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
{/snippet}

{#if rows.length > 0}
  <!-- One control, not one per row: every row here leads to the same window, filtered
       to the same branch, so per-row buttons would be N tab stops with N identical
       actions. The window is always openable, so there is no disabled state. -->
  <button
    type="button"
    class="group/card mx-2 my-1 px-2.5 py-1.5 rounded-lg border border-accent-muted flex flex-col gap-1 bg-transparent text-left font-inherit cursor-pointer hover:bg-hover"
    onclick={onActivate}
    aria-haspopup="dialog"
    title={cardHint}
  >
    {#if rows.length === 1 && rows[0]}
      {@const row = rows[0]}
      <span class="flex items-center gap-2 w-full min-w-0">
        <span
          class="flex-1 min-w-0 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint truncate"
          title={row.label}>{cardTitle} · {row.label}</span
        >
        {@render issueSuffix()}
        {#if row.timestamp != null}
          <!-- Grid overlap keeps the top line stable: HOVER replaces the date with the
               history icon. Deliberately not focus — focus is where a keyboard user IS,
               and blanking the date there loses it for good (Chromium renders `title` on
               hover only, never on :focus). Their affordance is the card's own focus
               ring, which the repository leaves at the Chromium default. -->
          <span class="grid shrink-0 items-center justify-items-end">
            <span
              class="col-start-1 row-start-1 transition-opacity duration-fast group-hover/card:opacity-0"
              >{@render stamp(row)}</span
            >
            <span
              class="col-start-1 row-start-1 flex items-center justify-end text-text-muted opacity-0 pointer-events-none transition-opacity duration-fast group-hover/card:opacity-100"
              aria-hidden="true"
            >
              <History size={11} />
            </span>
          </span>
        {:else}
          <!-- No date to displace here, so focus keeps its reveal: the asymmetry with
               the branch above is the point — that one would have to hide information
               to show this icon, this one costs nothing. -->
          <span
            class="flex items-center justify-center text-text-muted opacity-0 transition-opacity duration-fast group-hover/card:opacity-100 group-focus-within/card:opacity-100 shrink-0"
            aria-hidden="true"
          >
            <History size={11} />
          </span>
        {/if}
      </span>
      {@render statusLine(row)}
      {@render details(row)}
    {:else}
      <span class="flex items-center gap-2 w-full min-w-0">
        <span
          class="flex-1 min-w-0 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint truncate"
          >{cardTitle}</span
        >
        {@render issueSuffix()}
        <!-- Once for the card, not once per row: a single group/card would otherwise
             blank every row's date at the same time to show one affordance. -->
        <span
          class="flex items-center justify-center text-text-muted opacity-0 transition-opacity duration-fast group-hover/card:opacity-100 group-focus-within/card:opacity-100 shrink-0"
          aria-hidden="true"
        >
          <History size={11} />
        </span>
      </span>
      {#each rows as row (row.id)}
        <span class="flex flex-col gap-1 w-full">
          <span class="flex items-center gap-2 w-full min-w-0">
            <span
              class="flex-1 min-w-0 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint truncate"
              title={row.label}>{row.label}</span
            >
            {#if row.timestamp != null}
              <span class="shrink-0">{@render stamp(row)}</span>
            {/if}
          </span>
          {@render statusLine(row)}
          {@render details(row)}
        </span>
      {/each}
    {/if}
  </button>
{/if}
