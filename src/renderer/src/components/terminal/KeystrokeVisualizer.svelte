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
    filter: drop-shadow(0 0 4px var(--c-accent-muted));
  }

  .keystroke-overlay.blazing {
    filter: drop-shadow(0 0 8px var(--c-blazing-drop));
    animation: container-pulse 0.4s ease-in-out infinite alternate;
  }

  .keystroke-overlay.inferno {
    filter: drop-shadow(0 0 12px var(--c-inferno-drop));
    animation: container-pulse-inferno 0.3s ease-in-out infinite alternate;
  }

  .key-badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    background: var(--c-hover-strong);
    border: 1px solid var(--c-active);
    border-radius: 4px;
    font-family: monospace;
    font-size: 12px;
    color: var(--c-text);
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
    background: var(--c-accent-bg);
    border-color: var(--c-accent-muted);
    color: var(--c-accent-text);
    box-shadow: 0 0 6px var(--c-accent-bg);
    animation: keystroke-pop-fast 2s ease forwards;
  }

  .key-badge.blazing {
    background: var(--c-blazing-bg);
    border-color: var(--c-blazing-border);
    color: var(--c-blazing-text);
    box-shadow:
      0 0 8px var(--c-blazing-glow),
      0 0 16px var(--c-blazing-glow-deep);
    animation: keystroke-pop-blazing 2s ease forwards;
  }

  .fire-indicator {
    font-size: 14px;
    animation: fire-bounce 0.3s ease-in-out infinite alternate;
  }

  .key-badge.inferno {
    background: var(--c-inferno-bg);
    border-color: var(--c-inferno-border);
    color: var(--c-inferno-text);
    box-shadow:
      0 0 10px var(--c-inferno-glow),
      0 0 20px var(--c-inferno-glow-deep);
    animation: keystroke-pop-inferno 2s ease forwards;
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

  @keyframes keystroke-pop-inferno {
    0% {
      opacity: 0;
      transform: scale(0.3) translateY(10px) rotate(-3deg);
    }
    5% {
      opacity: 1;
      transform: scale(1.2) translateY(0) rotate(1.5deg);
    }
    8% {
      transform: scale(0.95) rotate(-1deg);
    }
    12% {
      transform: scale(1.05) rotate(0.5deg);
    }
    16% {
      transform: scale(1) rotate(0);
    }
    75% {
      opacity: 1;
    }
    100% {
      opacity: 0;
      transform: scale(0.8) translateY(-5px);
    }
  }

  @keyframes container-pulse-inferno {
    from {
      filter: drop-shadow(0 0 8px var(--c-inferno-shadow));
    }
    to {
      filter: drop-shadow(0 0 16px var(--c-inferno-shadow-strong));
    }
  }

  @keyframes container-pulse {
    from {
      filter: drop-shadow(0 0 6px var(--c-blazing-shadow));
    }
    to {
      filter: drop-shadow(0 0 12px var(--c-blazing-shadow-strong));
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
    .key-badge.blazing,
    .key-badge.inferno {
      animation: none;
      opacity: 1;
    }

    .keystroke-overlay.blazing,
    .keystroke-overlay.inferno {
      animation: none;
    }

    .fire-indicator {
      animation: none;
    }
  }
</style>
