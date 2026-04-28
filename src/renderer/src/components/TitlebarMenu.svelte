<script lang="ts">
  import { showPreferences, showAbout } from '../lib/stores/dialogs.svelte'

  let open = $state(false)
  let buttonEl: HTMLButtonElement | undefined = $state()
  let dropdownTop = $state(0)
  let dropdownLeft = $state(0)

  function toggle(): void {
    if (!open && buttonEl) {
      const rect = buttonEl.getBoundingClientRect()
      dropdownTop = rect.bottom + 4
      dropdownLeft = rect.left
    }
    open = !open
  }

  function close(): void {
    open = false
  }

  function handleAction(action: () => void): void {
    close()
    action()
  }

  function handleWindowKeydown(e: KeyboardEvent): void {
    if (open && e.key === 'Escape') {
      e.preventDefault()
      close()
    }
  }

  // Move element to document.body to escape titlebar backdrop-filter stacking context
  function portal(node: HTMLElement): { destroy(): void } {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
  }
</script>

<div class="relative flex items-center app-no-drag">
  <button
    bind:this={buttonEl}
    class="flex items-center justify-center w-9 h-7 border-0 rounded-md bg-transparent text-text-secondary cursor-pointer transition-colors duration-base hover:bg-active hover:text-text"
    onclick={toggle}
    title="Menu"
    aria-label="Application menu"
  >
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
      <rect x="2" y="3" width="12" height="1.5" rx="0.5" />
      <rect x="2" y="7.25" width="12" height="1.5" rx="0.5" />
      <rect x="2" y="11.5" width="12" height="1.5" rx="0.5" />
    </svg>
  </button>
</div>

<svelte:window onkeydown={handleWindowKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-overlay" use:portal onclick={close}>
    <div
      class="fixed min-w-55 p-1 bg-bg-overlay border border-border rounded-lg shadow-popover z-popover"
      style="top: {dropdownTop}px; left: {dropdownLeft}px;"
      onclick={(e) => e.stopPropagation()}
    >
      <button
        class="flex items-center justify-between gap-4 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left whitespace-nowrap transition-colors duration-fast hover:bg-hover-strong"
        onclick={() => handleAction(() => window.api.newWindow())}
      >
        <span class="flex-1">New Window</span>
        <span class="text-text-muted text-xs">Ctrl+Shift+N</span>
      </button>

      <div class="h-px mx-1.5 my-1 bg-active"></div>

      <button
        class="flex items-center justify-between gap-4 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left whitespace-nowrap transition-colors duration-fast hover:bg-hover-strong"
        onclick={() => handleAction(showPreferences)}
      >
        <span class="flex-1">Settings…</span>
        <span class="text-text-muted text-xs">Ctrl+,</span>
      </button>

      <div class="h-px mx-1.5 my-1 bg-active"></div>

      <button
        class="flex items-center justify-between gap-4 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left whitespace-nowrap transition-colors duration-fast hover:bg-hover-strong"
        onclick={() => handleAction(() => window.api.checkForUpdates())}
      >
        <span class="flex-1">Check for Updates…</span>
      </button>
      <button
        class="flex items-center justify-between gap-4 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left whitespace-nowrap transition-colors duration-fast hover:bg-hover-strong"
        onclick={() => handleAction(showAbout)}
      >
        <span class="flex-1">About Canopy</span>
      </button>
      <button
        class="flex items-center justify-between gap-4 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left whitespace-nowrap transition-colors duration-fast hover:bg-hover-strong"
        onclick={() =>
          handleAction(() => window.api.openExternal('https://canopy.itsol.tech/privacy-policy'))}
      >
        <span class="flex-1">Privacy Policy</span>
      </button>
      <button
        class="flex items-center justify-between gap-4 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left whitespace-nowrap transition-colors duration-fast hover:bg-hover-strong"
        onclick={() => handleAction(() => window.api.openThirdPartyNotices())}
      >
        <span class="flex-1">Third-Party Licenses</span>
      </button>

      <div class="h-px mx-1.5 my-1 bg-active"></div>

      <button
        class="flex items-center justify-between gap-4 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left whitespace-nowrap transition-colors duration-fast hover:bg-hover-strong"
        onclick={() => handleAction(() => window.api.quit())}
      >
        <span class="flex-1">Quit</span>
      </button>
    </div>
  </div>
{/if}
