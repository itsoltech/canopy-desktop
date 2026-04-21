<script lang="ts">
  import { ChevronRight, Brain } from '@lucide/svelte'

  interface Props {
    content: string
    label?: string
    defaultOpen?: boolean
    durationMs?: number
  }

  let { content, label = 'Thinking', defaultOpen = false, durationMs }: Props = $props()

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
    <span class="thinking-label">{label}</span>
    {#if durationText}
      <span class="thinking-duration">{durationText}</span>
    {/if}
  </button>

  {#if open}
    <div class="thinking-body">{content}</div>
  {/if}
</section>

<style>
  .thinking {
    display: flex;
    flex-direction: column;
    margin: 6px 0;
    border: 1px dashed var(--c-border-subtle);
    border-radius: 6px;
    background: transparent;
    color: var(--c-text-secondary);
  }

  .thinking-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: transparent;
    border: none;
    color: var(--c-text-muted);
    font-size: 12px;
    font-style: italic;
    cursor: pointer;
    text-align: left;
    width: 100%;
  }

  .thinking-head:hover {
    background: var(--c-hover);
    color: var(--c-text-secondary);
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
    font-size: 10.5px;
    font-style: normal;
    color: var(--c-text-faint);
    font-variant-numeric: tabular-nums;
  }

  .thinking-body {
    padding: 0 12px 10px 30px;
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--c-text-secondary);
    white-space: pre-wrap;
    font-style: italic;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
