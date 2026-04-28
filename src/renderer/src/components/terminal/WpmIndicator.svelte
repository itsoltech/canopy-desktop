<script lang="ts">
  import { getWpm, getSessionStats, isSessionActive } from '../../lib/stores/wpmTracker.svelte'

  let { sessionId }: { sessionId: string } = $props()

  let currentWpm = $derived(getWpm(sessionId))
  let stats = $derived(getSessionStats(sessionId))
  let active = $derived(isSessionActive(sessionId))
  let showBadge = $derived(active && currentWpm > 0)

  let visible = $state(false)
  let fadeTimer: ReturnType<typeof setTimeout> | undefined

  $effect(() => {
    if (showBadge) {
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
  <div
    class="absolute bottom-1.5 right-2.5 flex items-baseline gap-1 px-2.5 py-1 bg-hover rounded-lg pointer-events-none transition-opacity duration-slower ease-out-expo z-pane-divider antialiased motion-reduce:transition-none"
    class:opacity-100={visible || showBadge}
    class:opacity-0={!(visible || showBadge)}
  >
    <span class="text-md font-medium tabular-nums text-text">{currentWpm}</span>
    <span class="text-2xs font-normal text-text-faint">wpm</span>
    {#if stats.peakWpm > 0}
      <span class="w-px h-2.5 bg-border mx-0.5 self-center"></span>
      <span class="text-2xs font-normal text-text-faint">{stats.peakWpm} peak</span>
    {/if}
  </div>
{/if}
