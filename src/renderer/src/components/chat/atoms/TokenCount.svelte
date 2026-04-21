<script lang="ts">
  interface Props {
    tokens: number
    label?: string
  }

  let { tokens, label = 'tokens' }: Props = $props()

  let formatted = $derived.by(() => {
    if (tokens < 1000) return tokens.toString()
    if (tokens < 1_000_000) return `${(tokens / 1000).toFixed(1)}k`
    return `${(tokens / 1_000_000).toFixed(1)}M`
  })
</script>

<span class="token-count">
  <span class="value">{formatted}</span>
  <span class="label">{label}</span>
</span>

<style>
  .token-count {
    display: inline-flex;
    align-items: baseline;
    gap: 3px;
    font-size: 11px;
    color: var(--c-text-faint);
    font-variant-numeric: tabular-nums;
  }

  .value {
    color: var(--c-text-muted);
    font-weight: 500;
  }
</style>
