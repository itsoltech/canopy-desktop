<script lang="ts">
  import {
    getWpm,
    getLastActivity,
    getSessionStats,
    isSessionActive,
  } from '../../lib/stores/wpmTracker.svelte'

  let { sessionId }: { sessionId: string } = $props()

  let visible = $state(false)
  let fadeTimer: ReturnType<typeof setTimeout> | undefined
  let pollTimer: ReturnType<typeof setInterval> | undefined
  let currentWpm = $state(0)
  let peakWpm = $state(0)
  let active = $state(false)

  $effect(() => {
    pollTimer = setInterval(() => {
      const wpm = getWpm(sessionId)
      const lastActivity = getLastActivity(sessionId)
      const stats = getSessionStats(sessionId)
      const now = Date.now()
      const recentlyTyped = now - lastActivity < 2500

      currentWpm = wpm
      peakWpm = stats.peakWpm
      active = isSessionActive(sessionId)

      if (active && recentlyTyped && wpm > 0) {
        visible = true
        clearTimeout(fadeTimer)
        fadeTimer = setTimeout(() => {
          visible = false
        }, 3000)
      }
    }, 400)

    return () => {
      clearInterval(pollTimer)
      clearTimeout(fadeTimer)
    }
  })
</script>

{#if active}
  <div class="wpm-badge" class:visible>
    <span class="wpm-number">{currentWpm}</span>
    <span class="wpm-label">wpm</span>
    {#if peakWpm > 0}
      <span class="wpm-sep"></span>
      <span class="wpm-peak">{peakWpm} peak</span>
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
    background: rgba(255, 255, 255, 0.06);
    border-radius: 6px;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.6s cubic-bezier(0.16, 1, 0.3, 1);
    z-index: 5;
    -webkit-font-smoothing: antialiased;
  }

  .wpm-badge.visible {
    opacity: 1;
  }

  .wpm-number {
    font-size: 13px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    color: rgba(255, 255, 255, 0.7);
    letter-spacing: -0.01em;
  }

  .wpm-label {
    font-size: 10px;
    font-weight: 400;
    color: rgba(255, 255, 255, 0.3);
    letter-spacing: 0.02em;
  }

  .wpm-sep {
    width: 1px;
    height: 10px;
    background: rgba(255, 255, 255, 0.1);
    margin: 0 2px;
    align-self: center;
  }

  .wpm-peak {
    font-size: 10px;
    font-weight: 400;
    color: rgba(255, 255, 255, 0.25);
    letter-spacing: 0.02em;
  }
</style>
