<script lang="ts">
  import { getWpm, getSessionStats, isSessionActive } from '../../lib/stores/wpmTracker.svelte'

  let { sessionId }: { sessionId: string } = $props()

  let currentWpm = $derived(getWpm(sessionId))
  let stats = $derived(getSessionStats(sessionId))
  let active = $derived(isSessionActive(sessionId))

  let visible = $state(false)
  let fadeTimer: ReturnType<typeof setTimeout> | undefined

  $effect(() => {
    if (currentWpm > 0 && active) {
      visible = true
      clearTimeout(fadeTimer)
      fadeTimer = setTimeout(() => {
        visible = false
      }, 3000)
    }

    return () => clearTimeout(fadeTimer)
  })
</script>

{#if active}
  <div class="wpm-badge" class:visible>
    <span class="wpm-number">{currentWpm}</span>
    <span class="wpm-label">wpm</span>
    {#if stats.peakWpm > 0}
      <span class="wpm-sep"></span>
      <span class="wpm-peak">{stats.peakWpm} peak</span>
    {/if}
  </div>
{/if}

<style>
  .wpm-badge {
    position: absolute;
    bottom: 6px;
    right: 10px;
    display: flex;
    align-items: baseline;
    gap: 4px;
    padding: 4px 10px;
    background: var(--c-hover);
    border-radius: 6px;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.6s cubic-bezier(0.16, 1, 0.3, 1);
    z-index: 5;
    -webkit-font-smoothing: antialiased;
  }

  @media (prefers-reduced-motion: reduce) {
    .wpm-badge {
      transition: none;
    }
  }

  .wpm-badge.visible {
    opacity: 1;
  }

  .wpm-number {
    font-size: 13px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    color: var(--c-text);
    letter-spacing: -0.01em;
  }

  .wpm-label {
    font-size: 10px;
    font-weight: 400;
    color: var(--c-text-faint);
    letter-spacing: 0.02em;
  }

  .wpm-sep {
    width: 1px;
    height: 10px;
    background: var(--c-border);
    margin: 0 2px;
    align-self: center;
  }

  .wpm-peak {
    font-size: 10px;
    font-weight: 400;
    color: var(--c-text-faint);
    letter-spacing: 0.02em;
  }
</style>
