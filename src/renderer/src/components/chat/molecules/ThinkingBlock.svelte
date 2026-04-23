<script lang="ts">
  import { cubicOut } from 'svelte/easing'
  import { slide } from 'svelte/transition'
  import { ChevronRight, Brain } from '@lucide/svelte'
  import TypingDots from '../atoms/TypingDots.svelte'
  import MarkdownContent from './MarkdownContent.svelte'

  interface Props {
    content: string
    label?: string
    defaultOpen?: boolean
    active?: boolean
    durationMs?: number
  }

  let {
    content,
    label = 'Thinking',
    defaultOpen = false,
    active = false,
    durationMs,
  }: Props = $props()

  let open = $state(defaultOpen)

  let durationText = $derived.by(() => {
    if (durationMs === undefined) return null
    if (durationMs < 1000) return `${durationMs}ms`
    return `${(durationMs / 1000).toFixed(1)}s`
  })
</script>

<section class="thinking" class:open>
  <button class="thinking-head" type="button" aria-expanded={open} onclick={() => (open = !open)}>
    <ChevronRight class="chevron" size={14} />
    <Brain size={12} />
    <span class="thinking-label">
      {#if active}
        <TypingDots {label} />
      {:else}
        {label}
      {/if}
    </span>
    {#if durationText}
      <span class="thinking-duration">{durationText}</span>
    {/if}
  </button>

  {#if open}
    <div class="thinking-body" transition:slide={{ duration: 180, easing: cubicOut }}>
      <MarkdownContent {content} />
    </div>
  {/if}
</section>

<style>
  .thinking {
    display: flex;
    flex-direction: column;
    margin: 0;
    border: 1px solid transparent;
    border-left: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    color: var(--c-text-secondary);
    font-family: inherit;
    font-size: 0.95em;
    transition:
      border-color 0.14s ease,
      background-color 0.14s ease,
      opacity 0.14s ease;
  }

  .thinking:not(.open) {
    opacity: 0.55;
  }

  .thinking:hover,
  .thinking:focus-within {
    border-color: var(--c-border-subtle);
    border-left-color: var(--c-warning);
    background: color-mix(in srgb, var(--c-warning) 3%, transparent);
    opacity: 1;
  }

  .thinking-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px 5px 0;
    background: transparent;
    border: none;
    color: var(--c-text-muted);
    font-size: 1em;
    font-style: normal;
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition:
      color 0.14s ease,
      padding-left 0.14s ease;
  }

  .thinking-head:hover {
    background: color-mix(in srgb, var(--c-hover) 70%, transparent);
    color: var(--c-text-secondary);
  }

  .thinking:hover .thinking-head,
  .thinking:focus-within .thinking-head,
  .thinking.open .thinking-head {
    padding-left: 8px;
  }

  .thinking-head:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 2px var(--c-focus-ring);
  }

  .thinking-head :global(.chevron) {
    transition: transform 0.15s ease;
    flex-shrink: 0;
  }

  .thinking.open .thinking-head :global(.chevron) {
    transform: rotate(90deg);
  }

  .thinking-label {
    flex: 1;
    font-style: italic;
  }

  .thinking-duration {
    font-size: 0.8em;
    font-style: normal;
    color: var(--c-text-faint);
    font-variant-numeric: tabular-nums;
  }

  .thinking-body {
    padding: 6px 10px 8px 30px;
    font-size: 0.92em;
    line-height: 1.45;
    color: var(--c-text-secondary);
    white-space: pre-wrap;
    font-style: normal;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
