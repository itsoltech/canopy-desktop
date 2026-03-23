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
    background: rgba(0, 0, 0, 0.6);
    z-index: 10;
  }

  .exit-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .exit-text {
    color: rgba(255, 255, 255, 0.6);
    font-size: 13px;
  }

  .restart-btn {
    padding: 6px 16px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.8);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .restart-btn:hover {
    background: rgba(255, 255, 255, 0.14);
    border-color: rgba(255, 255, 255, 0.3);
  }
</style>
