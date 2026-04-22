<script lang="ts">
  import ModelBadge from '../atoms/ModelBadge.svelte'
  import TokenCount from '../atoms/TokenCount.svelte'

  interface Props {
    model?: string
    tokens?: number
    tokensIn?: number
    tokensOut?: number
    cacheReadInputTokens?: number
    cacheCreationInputTokens?: number
    costUsd?: number
    elapsedMs?: number
  }

  let {
    model,
    tokens,
    tokensIn,
    tokensOut,
    cacheReadInputTokens,
    cacheCreationInputTokens,
    costUsd,
    elapsedMs,
  }: Props = $props()

  let hasSplit = $derived(tokensIn !== undefined || tokensOut !== undefined)
  let splitTotal = $derived((tokensIn ?? 0) + (tokensOut ?? 0))
  let hasAnyTokens = $derived(hasSplit || tokens !== undefined)
  let hasCache = $derived(
    (cacheReadInputTokens !== undefined && cacheReadInputTokens > 0) ||
      (cacheCreationInputTokens !== undefined && cacheCreationInputTokens > 0),
  )

  let elapsedText = $derived.by(() => {
    if (elapsedMs === undefined) return null
    if (elapsedMs < 1000) return `${elapsedMs}ms`
    const s = elapsedMs / 1000
    if (s < 60) return `${s.toFixed(s < 10 ? 1 : 0)}s`
    const m = Math.floor(s / 60)
    const rem = Math.floor(s % 60)
    return `${m}m ${rem}s`
  })

  let costText = $derived.by(() => {
    if (costUsd === undefined) return null
    if (costUsd < 0.01) return `$${costUsd.toFixed(4)}`
    return `$${costUsd.toFixed(2)}`
  })
</script>

<footer class="message-meta">
  {#if model}
    <ModelBadge {model} />
  {/if}
  {#if hasSplit}
    {#if model}<span class="sep">·</span>{/if}
    <span class="split" aria-label="Input tokens">
      <span class="arrow">↓</span>
      <TokenCount tokens={tokensIn ?? 0} label="in" />
    </span>
    <span class="sep">·</span>
    <span class="split" aria-label="Output tokens">
      <span class="arrow">↑</span>
      <TokenCount tokens={tokensOut ?? 0} label="out" />
    </span>
    <span class="sep">·</span>
    <TokenCount tokens={splitTotal} label="total" />
  {:else if tokens !== undefined}
    {#if model}<span class="sep">·</span>{/if}
    <TokenCount {tokens} />
  {/if}
  {#if hasCache}
    {#if hasAnyTokens}<span class="sep">·</span>{/if}
    {#if cacheReadInputTokens !== undefined && cacheReadInputTokens > 0}
      <span class="cache" title="Cache-read input tokens">
        <span class="arrow">⚡</span>
        <TokenCount tokens={cacheReadInputTokens} label="cache" />
      </span>
    {/if}
    {#if cacheCreationInputTokens !== undefined && cacheCreationInputTokens > 0}
      {#if cacheReadInputTokens !== undefined && cacheReadInputTokens > 0}
        <span class="sep">·</span>
      {/if}
      <span class="cache" title="Cache-creation input tokens">
        <span class="arrow">✚</span>
        <TokenCount tokens={cacheCreationInputTokens} label="cache+" />
      </span>
    {/if}
  {/if}
  {#if costText}
    {#if model || hasAnyTokens || elapsedText}<span class="sep">·</span>{/if}
    <span class="cost">{costText}</span>
  {/if}
  {#if elapsedText}
    {#if model || hasAnyTokens || costText}<span class="sep">·</span>{/if}
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

  .split,
  .cache {
    display: inline-flex;
    align-items: baseline;
    gap: 3px;
  }

  .arrow {
    color: var(--c-text-faint);
    font-size: 10.5px;
    line-height: 1;
  }

  .elapsed {
    color: var(--c-text-muted);
  }

  .cost {
    color: var(--c-text-muted);
  }
</style>
