<script lang="ts">
  import type { AiSessionInfo } from '../../lib/stores/tabs.svelte'

  let {
    sessions,
    onSelect,
    onClose,
  }: {
    sessions: AiSessionInfo[]
    onSelect: (sessionId: string) => void
    onClose: () => void
  } = $props()

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="picker-overlay" onclick={onClose} onkeydown={handleKeydown}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="picker" onclick={(e) => e.stopPropagation()}>
    <div class="picker-title">Send to</div>
    {#each sessions as s (s.sessionId)}
      <button class="picker-item" onclick={() => onSelect(s.sessionId)}>
        <span class="picker-name">{s.tabName}</span>
        <span class="picker-status">{s.status}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .picker-overlay {
    position: fixed;
    inset: 0;
    z-index: 1002;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.4);
  }

  .picker {
    min-width: 220px;
    padding: 8px;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6);
  }

  .picker-title {
    padding: 4px 8px 8px;
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.5);
  }

  .picker-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.8);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }

  .picker-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .picker-name {
    flex: 1;
  }

  .picker-status {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
  }
</style>
