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

  // Move element to document.body to escape titlebar backdrop-filter stacking context
  function portal(node: HTMLElement): { destroy(): void } {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
  }
</script>

<div class="menu-wrapper">
  <button
    bind:this={buttonEl}
    class="hamburger"
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

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="titlebar-menu-overlay" use:portal onclick={close}>
    <div
      class="titlebar-menu-dropdown"
      style="top: {dropdownTop}px; left: {dropdownLeft}px;"
      onclick={(e) => e.stopPropagation()}
    >
      <button class="menu-item" onclick={() => handleAction(() => window.api.newWindow())}>
        <span class="label">New Window</span>
        <span class="shortcut">Ctrl+Shift+N</span>
      </button>

      <div class="separator"></div>

      <button class="menu-item" onclick={() => handleAction(showPreferences)}>
        <span class="label">Settings…</span>
        <span class="shortcut">Ctrl+,</span>
      </button>

      <div class="separator"></div>

      <button class="menu-item" onclick={() => handleAction(() => window.api.checkForUpdates())}>
        <span class="label">Check for Updates…</span>
      </button>
      <button class="menu-item" onclick={() => handleAction(showAbout)}>
        <span class="label">About Canopy</span>
      </button>
      <button
        class="menu-item"
        onclick={() =>
          handleAction(() => window.api.openExternal('https://canopy.itsol.tech/privacy-policy'))}
      >
        <span class="label">Privacy Policy</span>
      </button>
      <button
        class="menu-item"
        onclick={() => handleAction(() => window.api.openThirdPartyNotices())}
      >
        <span class="label">Third-Party Licenses</span>
      </button>

      <div class="separator"></div>

      <button class="menu-item" onclick={() => handleAction(() => window.api.quit())}>
        <span class="label">Quit</span>
      </button>
    </div>
  </div>
{/if}

<style>
  .menu-wrapper {
    position: relative;
    display: flex;
    align-items: center;
    app-region: no-drag;
  }

  .hamburger {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 28px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s;
  }

  .hamburger:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.9);
  }

  :global(.titlebar-menu-overlay) {
    position: fixed;
    inset: 0;
    z-index: 10000;
  }

  :global(.titlebar-menu-dropdown) {
    position: fixed;
    min-width: 220px;
    padding: 4px;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }

  .menu-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    width: 100%;
    padding: 6px 10px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.8);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    white-space: nowrap;
    transition: background 0.1s;
  }

  .menu-item:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .label {
    flex: 1;
  }

  .shortcut {
    color: rgba(255, 255, 255, 0.35);
    font-size: 11px;
  }

  .separator {
    height: 1px;
    margin: 4px 6px;
    background: rgba(255, 255, 255, 0.08);
  }
</style>
