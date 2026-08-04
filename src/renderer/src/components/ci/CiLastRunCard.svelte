<script lang="ts">
  import { ExternalLink } from '@lucide/svelte'
  import type { CiJobStatus } from '../../lib/ci/types'
  import { ciRunChip, ciRunStatusTextClass } from '../../lib/ci/status'

  let { rows, branch }: { rows: CiJobStatus[]; branch: string } = $props()
</script>

<div class="mx-2 my-1 rounded-lg border border-border-subtle overflow-hidden">
  {#each rows as row (row.jobId)}
    {@const status = ciRunChip(row)}
    <button
      type="button"
      class="group w-full px-2.5 py-1.5 border-0 bg-transparent text-left hover:bg-hover flex items-center gap-2 aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
      aria-disabled={!row.run?.webUrl}
      onclick={() => row.run?.webUrl && window.api.openExternal(row.run.webUrl)}
      title={row.error || row.run?.statusText || `Latest ${branch} run`}
    >
      <span class="flex-1 min-w-0 flex flex-col">
        <span class="text-xs text-text truncate">{row.label}</span>
        {#if row.run?.statusText}
          <span class="text-2xs truncate {ciRunStatusTextClass(row.run)}">{row.run.statusText}</span
          >
        {:else if row.error}
          <span class="text-2xs text-warning-text truncate">{row.error}</span>
        {/if}
      </span>
      {#if row.run?.number}<span class="font-mono text-2xs text-text-faint">#{row.run.number}</span
        >{/if}
      <span class="px-1.5 py-px rounded-md text-2xs {status.cls}">{status.label}</span>
      {#if row.run?.webUrl}<ExternalLink size={10} class="opacity-50" />{/if}
    </button>
  {/each}
</div>
