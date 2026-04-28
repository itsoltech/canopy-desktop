<script lang="ts">
  import { ChevronDown, ChevronRight } from '@lucide/svelte'
  import type { Snippet } from 'svelte'

  let {
    name,
    agentDisplay,
    expanded,
    dimmed,
    onToggle,
    children,
  }: {
    name: string
    agentDisplay: string
    expanded: boolean
    dimmed: boolean
    onToggle: () => void
    children: Snippet
  } = $props()
</script>

<div
  class="flex flex-col border-t border-border-subtle first:border-t-0 transition-opacity duration-fast"
  class:opacity-30={dimmed}
>
  <button
    type="button"
    class="flex items-center gap-2 w-full px-1 py-2 border-0 bg-transparent text-left cursor-pointer hover:bg-row-hover rounded-md"
    onclick={onToggle}
  >
    {#if expanded}
      <ChevronDown size={13} class="shrink-0 text-text-muted" />
    {:else}
      <ChevronRight size={13} class="shrink-0 text-text-muted" />
    {/if}
    <span class="text-md text-text min-w-30 truncate">{name}</span>
    <span class="text-xs text-text-muted truncate flex-1">{agentDisplay}</span>
  </button>
  {#if expanded}
    <div class="pl-6 pr-1 pb-3 flex flex-col gap-2.5">
      {@render children()}
    </div>
  {/if}
</div>
