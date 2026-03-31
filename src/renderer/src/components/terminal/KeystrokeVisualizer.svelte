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
  >
    {#each keystrokes as ks (ks.id)}
      <span
        class="key-badge"
        class:fast={intensity === 'fast'}
        class:blazing={intensity === 'blazing'}
      >
        {ks.label}
      </span>
    {/each}
    {#if intensity === 'blazing'}
      <span class="fire-indicator">&#xFE0F;</span>
    {/if}
  </div>
{/if}

<style>
  .keystroke-overlay {
    position: absolute;
    bottom: 6px;
    left: 10px;
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    pointer-events: none;
    z-index: 5;
    transition: filter 0.3s ease;
  }

  .keystroke-overlay.fast {
    filter: drop-shadow(0 0 4px rgba(116, 192, 252, 0.4));
  }

  .keystroke-overlay.blazing {
    filter: drop-shadow(0 0 8px rgba(255, 140, 50, 0.6));
    animation: container-pulse 0.4s ease-in-out infinite alternate;
  }

  .key-badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    font-family: monospace;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.85);
    white-space: nowrap;
    animation: keystroke-pop 2s ease forwards;
    -webkit-font-smoothing: antialiased;
    transition:
      background 0.2s ease,
      border-color 0.2s ease,
      color 0.2s ease,
      box-shadow 0.2s ease;
  }

  .key-badge.fast {
    background: rgba(116, 192, 252, 0.15);
    border-color: rgba(116, 192, 252, 0.3);
    color: rgba(180, 220, 255, 0.95);
    box-shadow: 0 0 6px rgba(116, 192, 252, 0.2);
    animation: keystroke-pop-fast 2s ease forwards;
  }

  .key-badge.blazing {
    background: rgba(255, 140, 50, 0.2);
    border-color: rgba(255, 140, 50, 0.4);
    color: rgba(255, 200, 140, 1);
    box-shadow:
      0 0 8px rgba(255, 140, 50, 0.3),
      0 0 16px rgba(255, 80, 20, 0.15);
    animation: keystroke-pop-blazing 2s ease forwards;
  }

  .fire-indicator {
    font-size: 14px;
    animation: fire-bounce 0.3s ease-in-out infinite alternate;
  }

  @keyframes keystroke-pop {
    0% {
      opacity: 0;
      transform: scale(0.7) translateY(4px);
    }
    10% {
      opacity: 1;
      transform: scale(1.05) translateY(0);
    }
    15% {
      transform: scale(1);
    }
    75% {
      opacity: 1;
    }
    100% {
      opacity: 0;
      transform: scale(0.95) translateY(-2px);
    }
  }

  @keyframes keystroke-pop-fast {
    0% {
      opacity: 0;
      transform: scale(0.5) translateY(6px);
    }
    8% {
      opacity: 1;
      transform: scale(1.1) translateY(0);
    }
    12% {
      transform: scale(1);
    }
    75% {
      opacity: 1;
    }
    100% {
      opacity: 0;
      transform: scale(0.9) translateY(-3px);
    }
  }

  @keyframes keystroke-pop-blazing {
    0% {
      opacity: 0;
      transform: scale(0.4) translateY(8px) rotate(-2deg);
    }
    6% {
      opacity: 1;
      transform: scale(1.15) translateY(0) rotate(1deg);
    }
    10% {
      transform: scale(0.98) rotate(-0.5deg);
    }
    14% {
      transform: scale(1) rotate(0);
    }
    75% {
      opacity: 1;
    }
    100% {
      opacity: 0;
      transform: scale(0.85) translateY(-4px);
    }
  }

  @keyframes container-pulse {
    from {
      filter: drop-shadow(0 0 6px rgba(255, 140, 50, 0.5));
    }
    to {
      filter: drop-shadow(0 0 12px rgba(255, 80, 20, 0.7));
    }
  }

  @keyframes fire-bounce {
    from {
      transform: translateY(0) scale(1);
    }
    to {
      transform: translateY(-3px) scale(1.15);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .key-badge,
    .key-badge.fast,
    .key-badge.blazing {
      animation: none;
      opacity: 1;
    }

    .keystroke-overlay.blazing {
      animation: none;
    }

    .fire-indicator {
      animation: none;
    }
  }
</style>
