<script lang="ts">
  interface Props {
    label?: string
    /** ms per frame — default matches cli-spinners "dots". */
    interval?: number
  }

  let { label = 'Typing', interval = 80 }: Props = $props()

  const FRAMES = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'] as const

  let index = $state(0)

  $effect(() => {
    const reduce =
      typeof window !== 'undefined' &&
      window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
    if (reduce) return

    const t = setInterval(() => {
      index = (index + 1) % FRAMES.length
    }, interval)
    return () => clearInterval(t)
  })
</script>

<span class="spinner" role="status" aria-label={label}>{FRAMES[index]}</span>

<style>
  .spinner {
    display: inline-block;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    color: var(--c-text-muted);
    line-height: 1;
    /* Lock width so the glyph swap doesn't jitter surrounding layout. */
    width: 1ch;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
</style>
