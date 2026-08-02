<script lang="ts">
  import { ExternalLink } from '@lucide/svelte'
  import { ciChip } from '../../lib/ci/status'
  import type { CiBuildTypeStatus } from '../../lib/ci/types'

  // Newest build of the ACTIVE worktree's branch, per configured job. With a single
  // job the WHOLE card is one click target — hover OR keyboard focus anywhere
  // reveals the corner open icon and lights up the build number.
  //
  // Rows without a build use aria-disabled (not disabled): a disabled button leaves
  // the tab order and Chromium suppresses its title tooltip, which would make the
  // "why is this unavailable" reason unreachable by any input modality. onclick
  // guards on row.build, so nothing can be activated.

  let { rows, branch }: { rows: CiBuildTypeStatus[]; branch: string } = $props()
</script>

{#snippet lastJobLine(row: CiBuildTypeStatus, showJobLabel: boolean)}
  <!-- Shared line of both variants. Reveals are paired hover + focus-within so
       keyboard focus gets the same affordance. -->
  {@const chip = ciChip(row)}
  <span class="flex items-center gap-2 w-full text-sm text-text">
    <span class="flex-1 min-w-0 truncate font-mono text-xs text-text-muted">{branch}</span>
    {#if showJobLabel}
      <span class="text-2xs text-text-faint truncate max-w-24">{row.label}</span>
    {/if}
    {#if row.build}
      <span
        class="font-mono text-2xs text-text-secondary flex-shrink-0 underline-offset-2 group-hover/card:text-accent-text group-focus-within/card:text-accent-text group-hover/card:underline group-focus-within/card:underline"
        >#{row.build.number}</span
      >
    {/if}
    <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 {chip.cls}">{chip.label}</span>
    {#if row.error}
      <!-- The failure reason must not depend on a tooltip — disabled/aria-disabled
           tooltips are unreliable, and screen readers don't read title by default. -->
      <span class="sr-only">{row.error}</span>
    {/if}
  </span>
  {#if row.error}
    <!-- Visible route to the reason for sighted keyboard users: title never fires on
         :focus in Chromium. aria-hidden keeps it from being read twice next to the
         sr-only copy above. -->
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
    aria-disabled={!row.build}
    onclick={() => row.build && window.api.openExternal(row.build.webUrl)}
    title={row.build
      ? `${row.label} — open build #${row.build.number} in TeamCity`
      : row.error
        ? `${row.label} — status unavailable: ${row.error}`
        : `No builds of ${row.label} for this branch yet`}
  >
    <span class="flex items-center gap-2 w-full">
      <span
        class="flex-1 min-w-0 text-2xs font-semibold uppercase tracking-caps-tight text-text-faint truncate"
        title={row.label}>Last job · {row.label}</span
      >
      {#if row.build}
        <span
          class="flex items-center justify-center text-text-muted opacity-0 transition-opacity duration-fast group-hover/card:opacity-100 group-focus-within/card:opacity-100 flex-shrink-0"
          aria-hidden="true"
        >
          <ExternalLink size={11} />
        </span>
      {/if}
    </span>
    {@render lastJobLine(row, false)}
  </button>
{:else if rows.length > 1}
  <!-- Multiple configured jobs: header + one click target per job. -->
  <div class="mx-2 my-1 px-2.5 py-1.5 rounded-lg border border-accent-muted flex flex-col gap-1">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint truncate"
      >Last job</span
    >
    {#each rows as row (row.buildTypeId)}
      <button
        type="button"
        class="group/card flex flex-col gap-1 w-full border-0 bg-transparent p-0 text-sm text-text font-inherit text-left cursor-pointer aria-disabled:cursor-default"
        aria-disabled={!row.build}
        onclick={() => row.build && window.api.openExternal(row.build.webUrl)}
        title={row.build
          ? `${row.label} — open build #${row.build.number} in TeamCity`
          : row.error
            ? `${row.label} — status unavailable: ${row.error}`
            : `No builds of ${row.label} for this branch yet`}
      >
        {@render lastJobLine(row, true)}
      </button>
    {/each}
  </div>
{/if}
