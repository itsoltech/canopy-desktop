<script lang="ts">
  import ModelBadge from '../atoms/ModelBadge.svelte'
  import TokenCount from '../atoms/TokenCount.svelte'

  interface Props {
    model?: string
    tokens?: number
    elapsedMs?: number
  }

  let { model, tokens, elapsedMs }: Props = $props()

  let elapsedText = $derived.by(() => {
    if (elapsedMs === undefined) return null
    if (elapsedMs < 1000) return `${elapsedMs}ms`
    const s = elapsedMs / 1000
    if (s < 60) return `${s.toFixed(s < 10 ? 1 : 0)}s`
    const m = Math.floor(s / 60)
    const rem = Math.floor(s % 60)
    return `${m}m ${rem}s`
  })
</script>

<footer class="message-meta">
  {#if model}
    <ModelBadge {model} />
  {/if}
  {#if tokens !== undefined}
    {#if model}<span class="sep">·</span>{/if}
    <TokenCount {tokens} />
  {/if}
  {#if elapsedText}
    {#if model || tokens !== undefined}<span class="sep">·</span>{/if}
    <span class="elapsed">{elapsedText}</span>
  {/if}
</footer>

<style>
  .message-meta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--c-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .sep {
    color: var(--c-text-faint);
  }

  .elapsed {
    color: var(--c-text-muted);
  }
</style>
