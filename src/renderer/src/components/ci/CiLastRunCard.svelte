<script lang="ts">
  import { ExternalLink } from '@lucide/svelte'
  import type { CiJobStatus } from '../../lib/ci/types'

  let { rows, branch }: { rows: CiJobStatus[]; branch: string } = $props()

  function chip(row: CiJobStatus): { label: string; cls: string } {
    const run = row.run
    if (row.error) return { label: 'Error', cls: 'bg-warning-bg text-warning-text' }
    if (!run) return { label: 'No runs', cls: 'bg-active text-text-muted' }
    if (run.state === 'waiting') return { label: 'Waiting', cls: 'bg-warning-bg text-warning-text' }
    if (run.state === 'running') return { label: 'Running', cls: 'bg-accent-bg text-accent-text' }
    if (run.state === 'queued') return { label: 'Queued', cls: 'bg-active text-text-muted' }
    if (run.conclusion === 'success') return { label: 'Success', cls: 'bg-success-bg text-success' }
    if (run.conclusion === 'failure')
      return { label: 'Failed', cls: 'bg-danger-bg text-danger-text' }
    if (run.conclusion === 'cancelled')
      return { label: 'Cancelled', cls: 'bg-active text-text-muted' }
    if (run.conclusion === 'neutral') return { label: 'Neutral', cls: 'bg-active text-text-muted' }
    return { label: 'Unknown', cls: 'bg-active text-text-muted' }
  }
</script>

<div class="mx-2 my-1 rounded-lg border border-border-subtle overflow-hidden">
  {#each rows as row (row.jobId)}
    {@const status = chip(row)}
    <button
      type="button"
      class="group w-full px-2.5 py-1.5 border-0 bg-transparent text-left hover:bg-hover flex items-center gap-2 disabled:cursor-default"
      disabled={!row.run?.webUrl}
      onclick={() => row.run?.webUrl && window.api.openExternal(row.run.webUrl)}
      title={row.error || row.run?.statusText || `Latest ${branch} run`}
    >
      <span class="flex-1 min-w-0 flex flex-col">
        <span class="text-xs text-text truncate">{row.label}</span>
        {#if row.run?.statusText}
          <span
            class="text-2xs truncate {row.run.conclusion === 'success'
              ? 'text-success'
              : row.run.conclusion === 'failure'
                ? 'text-danger-text'
                : 'text-text-muted'}">{row.run.statusText}</span
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
