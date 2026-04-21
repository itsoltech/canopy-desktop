<script lang="ts">
  import Tooltip from '../../shared/Tooltip.svelte'

  interface Props {
    timestamp: Date | string | number
  }

  let { timestamp }: Props = $props()

  let date = $derived(new Date(timestamp))
  let now = $state(Date.now())

  $effect(() => {
    const t = setInterval(() => {
      now = Date.now()
    }, 30_000)
    return () => clearInterval(t)
  })

  let relative = $derived.by(() => {
    const diff = Math.max(0, now - date.getTime())
    const s = Math.floor(diff / 1000)
    if (s < 45) return 'just now'
    const m = Math.floor(s / 60)
    if (m < 60) return `${m}m ago`
    const h = Math.floor(m / 60)
    if (h < 24) return `${h}h ago`
    const d = Math.floor(h / 24)
    if (d < 7) return `${d}d ago`
    return date.toLocaleDateString()
  })

  let absolute = $derived(date.toLocaleString())
</script>

<Tooltip text={absolute}>
  <time class="timestamp" datetime={date.toISOString()}>{relative}</time>
</Tooltip>

<style>
  .timestamp {
    display: inline-flex;
    align-items: center;
    font-size: 11px;
    color: var(--c-text-muted);
    font-variant-numeric: tabular-nums;
    cursor: default;
  }
</style>
