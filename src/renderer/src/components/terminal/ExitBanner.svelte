<script lang="ts">
  let {
    exitCode,
    onRestart,
  }: {
    exitCode: number | null
    onRestart: () => void
  } = $props()
</script>

<div class="exit-banner">
  <div class="exit-content">
    <span class="exit-text">
      Process exited{exitCode !== null ? ` (code ${exitCode})` : ''}
    </span>
    <button class="restart-btn" onclick={onRestart}>Restart</button>
  </div>
</div>

<style>
  .exit-banner {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--c-scrim);
    z-index: 10;
  }

  .exit-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .exit-text {
    color: var(--c-text-secondary);
    font-size: 13px;
  }

  .restart-btn {
    padding: 6px 16px;
    border: 1px solid var(--c-text-faint);
    border-radius: 6px;
    background: var(--c-active);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .restart-btn:hover {
    background: var(--c-hover-strong);
    border-color: var(--c-text-faint);
  }
</style>
