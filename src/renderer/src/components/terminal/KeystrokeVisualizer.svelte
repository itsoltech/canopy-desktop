<script lang="ts">
  import { getKeystrokes, getIntensity } from '../../lib/stores/keystrokeVisualizer.svelte'

  let { sessionId }: { sessionId: string } = $props()

  let keystrokes = $derived(getKeystrokes(sessionId))
  let intensity = $derived(getIntensity(sessionId))
</script>

{#if keystrokes.length > 0}
  <div
    class="keystroke-overlay"
    class:fast={intensity === 'fast'}
    class:blazing={intensity === 'blazing'}
    class:inferno={intensity === 'inferno'}
  >
    {#each keystrokes as ks (ks.id)}
      <span
        class="key-badge"
        class:fast={intensity === 'fast'}
        class:blazing={intensity === 'blazing'}
        class:inferno={intensity === 'inferno'}
      >
        {ks.label}
      </span>
    {/each}
  </div>
{/if}
