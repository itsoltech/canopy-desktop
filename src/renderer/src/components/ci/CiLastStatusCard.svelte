<script lang="ts">
  import { ExternalLink } from '@lucide/svelte'

  interface LastStatusRow {
    id: string
    label: string
    number?: string
    webUrl?: string
    statusText?: string
    statusTextClass?: string
    error?: string
    chip: { label: string; cls: string }
  }

  let {
    rows,
    branch,
    providerLabel,
  }: { rows: LastStatusRow[]; branch: string; providerLabel: string } = $props()
</script>

{#snippet statusLine(row: LastStatusRow, showJobLabel: boolean)}
  <span class="flex items-center gap-2 w-full text-sm text-text">
    <span class="flex-1 min-w-0 truncate font-mono text-xs text-text-muted">{branch}</span>
    {#if showJobLabel}
      <span class="text-2xs text-text-faint truncate max-w-24">{row.label}</span>
    {/if}
    {#if row.number}
      <span
        class="font-mono text-2xs text-text-secondary flex-shrink-0 underline-offset-2 group-hover/card:text-accent-text group-focus-within/card:text-accent-text group-hover/card:underline group-focus-within/card:underline"
        >#{row.number}</span
      >
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
      aria-hidden="true">{row.error}</span
    >
  {/if}
{/snippet}

{#if rows.length === 1 && rows[0]}
  {@const row = rows[0]}
  <button
    type="button"
    class="group/card mx-2 my-1 px-2.5 py-1.5 rounded-lg border border-accent-muted flex flex-col gap-1 bg-transparent text-left font-inherit cursor-pointer aria-disabled:cursor-default"
    aria-disabled={!row.webUrl}
    onclick={() => row.webUrl && window.api.openExternal(row.webUrl)}
    title={row.webUrl
      ? `${row.label} — open run${row.number ? ` #${row.number}` : ''} in ${providerLabel}`
      : row.error
        ? `${row.label} — status unavailable: ${row.error}`
        : row.number || row.statusText
          ? `${row.label} — run${row.number ? ` #${row.number}` : ''} cannot be opened in ${providerLabel}`
          : `No runs of ${row.label} for ${branch} yet`}
  >
    <span class="flex items-center gap-2 w-full">
      <span
        class="flex-1 min-w-0 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint truncate"
        title={row.label}>Last job · {row.label}</span
      >
      {#if row.webUrl}
        <span
          class="flex items-center justify-center text-text-muted opacity-0 transition-opacity duration-fast group-hover/card:opacity-100 group-focus-within/card:opacity-100 flex-shrink-0"
          aria-hidden="true"
        >
          <ExternalLink size={11} />
        </span>
      {/if}
    </span>
    {@render statusLine(row, false)}
  </button>
{:else if rows.length > 1}
  <div class="mx-2 my-1 px-2.5 py-1.5 rounded-lg border border-accent-muted flex flex-col gap-1">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint truncate"
      >Last job</span
    >
    {#each rows as row (row.id)}
      <button
        type="button"
        class="group/card flex flex-col gap-1 w-full border-0 bg-transparent p-0 text-sm text-text font-inherit text-left cursor-pointer aria-disabled:cursor-default"
        aria-disabled={!row.webUrl}
        onclick={() => row.webUrl && window.api.openExternal(row.webUrl)}
        title={row.webUrl
          ? `${row.label} — open run${row.number ? ` #${row.number}` : ''} in ${providerLabel}`
          : row.error
            ? `${row.label} — status unavailable: ${row.error}`
            : row.number || row.statusText
              ? `${row.label} — run${row.number ? ` #${row.number}` : ''} cannot be opened in ${providerLabel}`
              : `No runs of ${row.label} for ${branch} yet`}
      >
        {@render statusLine(row, true)}
      </button>
    {/each}
  </div>
{/if}
