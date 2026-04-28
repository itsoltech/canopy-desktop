<script lang="ts">
  import type { Snippet } from 'svelte'
  import { prefsSearch, matches } from './prefsSearch.svelte'

  type BadgeTone = 'warning' | 'accent' | 'neutral' | 'danger'

  interface Props {
    label: string
    help?: string
    search?: string
    layout?: 'inline' | 'stacked'
    badge?: { text: string; tone?: BadgeTone }
    children: Snippet
  }

  let { label, help, search, layout = 'inline', badge, children }: Props = $props()

  const haystack = $derived(`${label} ${help ?? ''} ${search ?? ''}`)
  const visible = $derived(prefsSearch.query.trim() === '' || matches(haystack))

  const badgeClasses: Record<BadgeTone, string> = {
    warning: 'bg-experimental-bg text-warning-text border border-experimental-border',
    accent: 'bg-accent-bg text-accent-text',
    neutral: 'bg-border-subtle text-text-muted',
    danger: 'bg-danger-bg text-danger-text',
  }
</script>

{#snippet labelBlock()}
  <span class="flex items-center gap-1.5 flex-wrap">
    <span class="text-md text-text leading-snug">{label}</span>
    {#if badge}
      <span
        class="inline-flex items-center text-2xs font-semibold uppercase tracking-caps-tight px-1.5 py-px rounded-md {badgeClasses[
          badge.tone ?? 'neutral'
        ]}"
      >
        {badge.text}
      </span>
    {/if}
  </span>
{/snippet}

{#if layout === 'inline'}
  <div
    class="flex items-start justify-between gap-6 py-3 border-t border-border-subtle first:border-t-0 first:pt-0 transition-opacity duration-fast"
    class:opacity-30={!visible}
  >
    <div class="flex flex-col flex-1 min-w-0 gap-0.5">
      {@render labelBlock()}
      {#if help}
        <span class="text-xs text-text-muted leading-snug max-w-[55ch]">{help}</span>
      {/if}
    </div>
    <div class="shrink-0 pt-0.5">
      {@render children()}
    </div>
  </div>
{:else}
  <div
    class="flex flex-col gap-2 py-3 border-t border-border-subtle first:border-t-0 first:pt-0 transition-opacity duration-fast"
    class:opacity-30={!visible}
  >
    <div class="flex flex-col gap-0.5">
      {@render labelBlock()}
      {#if help}
        <span class="text-xs text-text-muted leading-snug max-w-[55ch]">{help}</span>
      {/if}
    </div>
    <div>
      {@render children()}
    </div>
  </div>
{/if}
