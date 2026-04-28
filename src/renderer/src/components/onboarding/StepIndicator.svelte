<script lang="ts">
  interface Props {
    total: number
    current: number
  }

  let { total, current }: Props = $props()
</script>

<div class="dots" role="group" aria-label="Step {current + 1} of {total}">
  {#each Array.from({ length: total }, (_, k) => k) as i (i)}
    <div class="dot" class:active={i === current} class:completed={i < current}></div>
  {/each}
</div>

<style>
  .dots {
    display: flex;
    gap: 8px;
    justify-content: center;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-text-faint);
    transition:
      background 0.2s,
      transform 0.2s;
  }

  .dot.active {
    background: var(--color-accent);
    transform: scale(1.25);
  }

  .dot.completed {
    background: var(--color-accent-muted);
  }
</style>
