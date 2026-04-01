<script lang="ts">
  let {
    tmuxSessionName,
    onReattach,
    onKill,
  }: {
    tmuxSessionName: string
    onReattach: () => void
    onKill: () => void
  } = $props()
</script>

<div class="detached-overlay">
  <div class="detached-content">
    <span class="detached-text">Session detached</span>
    <span class="detached-name">{tmuxSessionName}</span>
    <span class="detached-hint">Still running in background</span>
    <div class="detached-actions">
      <button class="action-btn primary" onclick={onReattach}>Reattach</button>
      <button class="action-btn destructive" onclick={onKill}>Kill Session</button>
    </div>
  </div>
</div>

<style>
  .detached-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--c-scrim);
    z-index: 10;
  }

  .detached-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .detached-text {
    color: var(--c-text);
    font-size: 14px;
    font-weight: 500;
  }

  .detached-name {
    color: var(--c-text-faint);
    font-size: 11px;
    font-family: var(--font-mono, monospace);
  }

  .detached-hint {
    color: var(--c-text-secondary);
    font-size: 12px;
    margin-bottom: 4px;
  }

  .detached-actions {
    display: flex;
    gap: 8px;
  }

  .action-btn {
    padding: 6px 16px;
    border: 1px solid var(--c-text-faint);
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .action-btn.primary {
    background: var(--c-accent);
    color: var(--c-accent-text, #fff);
    border-color: var(--c-accent);
  }

  .action-btn.primary:hover {
    opacity: 0.9;
  }

  .action-btn.destructive {
    background: var(--c-active);
    color: var(--c-text);
  }

  .action-btn.destructive:hover {
    background: var(--c-hover-strong);
    border-color: var(--c-danger, #e55);
    color: var(--c-danger, #e55);
  }
</style>
