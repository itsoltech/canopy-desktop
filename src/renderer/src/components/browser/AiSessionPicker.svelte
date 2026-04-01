<script lang="ts">
  import { onMount } from 'svelte'
  import type { AiSessionInfo } from '../../lib/stores/tabs.svelte'

  let containerEl: HTMLDivElement | undefined = $state()

  let {
    sessions,
    onSelect,
    onClose,
  }: {
    sessions: AiSessionInfo[]
    onSelect: (sessionId: string) => void
    onClose: () => void
  } = $props()

  onMount(() => {
    containerEl?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="picker-overlay" onmousedown={onClose} onkeydown={handleKeydown}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="picker"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
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
    background: var(--c-scrim);
  }

  .picker {
    outline: none;
    min-width: 220px;
    padding: 8px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6);
  }

  .picker-title {
    padding: 4px 8px 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--c-text-secondary);
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
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }

  .picker-item:hover {
    background: var(--c-active);
  }

  .picker-name {
    flex: 1;
  }

  .picker-status {
    font-size: 11px;
    color: var(--c-text-muted);
  }
</style>
