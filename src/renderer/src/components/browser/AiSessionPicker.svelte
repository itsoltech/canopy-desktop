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

<div
  role="presentation"
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  onmousedown={onClose}
  onkeydown={handleKeydown}
>
  <div
    bind:this={containerEl}
    class="outline-none min-w-55 p-2 bg-bg-overlay border border-border rounded-xl shadow-modal"
    role="dialog"
    aria-modal="true"
    aria-label="Send to session"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="px-2 pt-1 pb-2 text-sm font-semibold text-text-secondary">Send to</div>
    {#each sessions as s (s.sessionId)}
      <button
        class="flex items-center justify-between gap-3 w-full px-2.5 py-2 border-0 rounded-md bg-transparent text-text text-md font-inherit cursor-pointer text-left hover:bg-active"
        onclick={() => onSelect(s.sessionId)}
      >
        <span class="flex-1">{s.tabName}</span>
        <span class="text-xs text-text-muted">{s.status}</span>
      </button>
    {/each}
  </div>
</div>
