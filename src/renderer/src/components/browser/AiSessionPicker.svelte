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
    // Restore focus to the element that opened the picker when it closes.
    const previouslyFocused = document.activeElement as HTMLElement | null
    containerEl?.focus()
    return () => previouslyFocused?.focus?.()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose()
      return
    }
    if (e.key === 'Tab' && containerEl) {
      // Trap focus within the picker so Tab doesn't walk into the obscured background.
      const focusable = containerEl.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement as HTMLElement | null
      if (e.shiftKey && (active === first || !containerEl.contains(active))) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  onmousedown={onClose}
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
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
