<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    RotateCw,
    Code,
    PanelBottom,
    PanelRight,
    Crosshair,
    Camera,
    X,
  } from 'lucide-svelte'

  let {
    url,
    canGoBack,
    canGoForward,
    isLoading,
    isDevToolsOpen,
    devToolsMode,
    pickMode = 'none',
    onNavigate,
    onBack,
    onForward,
    onReload,
    onToggleDevTools,
    onSwitchDevToolsMode,
    onStartElementPick,
    onStartRegionCapture,
    onCancelPick,
  }: {
    url: string
    canGoBack: boolean
    canGoForward: boolean
    isLoading: boolean
    isDevToolsOpen: boolean
    devToolsMode: 'bottom' | 'right'
    pickMode?: 'none' | 'element' | 'region'
    onNavigate: (url: string) => void
    onBack: () => void
    onForward: () => void
    onReload: () => void
    onToggleDevTools: () => void
    onSwitchDevToolsMode: () => void
    onStartElementPick: () => void
    onStartRegionCapture: () => void
    onCancelPick: () => void
  } = $props()

  let captureDropdownOpen = $state(false)

  function openDropdown(): void {
    captureDropdownOpen = true
    window.dispatchEvent(new CustomEvent('canopy:freeze-browsers'))
  }

  function closeDropdown(): void {
    captureDropdownOpen = false
    window.dispatchEvent(new CustomEvent('canopy:unfreeze-browsers'))
  }

  let inputValue = $state(url)
  let urlInput: HTMLInputElement | undefined = $state()

  // Sync input value when url changes externally (navigation)
  $effect(() => {
    if (document.activeElement !== urlInput) {
      inputValue = url
    }
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      onNavigate(inputValue)
      urlInput?.blur()
    } else if (event.key === 'Escape') {
      inputValue = url
      urlInput?.blur()
    }
  }

  function handleFocus(): void {
    urlInput?.select()
  }

  export function focusUrlBar(): void {
    urlInput?.focus()
    urlInput?.select()
  }
</script>

<div class="browser-toolbar">
  <div class="nav-buttons">
    <button class="nav-btn" disabled={!canGoBack} onclick={onBack} title="Back">
      <ArrowLeft size={14} />
    </button>
    <button class="nav-btn" disabled={!canGoForward} onclick={onForward} title="Forward">
      <ArrowRight size={14} />
    </button>
    <button class="nav-btn" class:loading={isLoading} onclick={onReload} title="Reload">
      <RotateCw size={14} />
    </button>
  </div>

  <input
    bind:this={urlInput}
    class="url-bar"
    type="text"
    bind:value={inputValue}
    onkeydown={handleKeydown}
    onfocus={handleFocus}
    placeholder="Enter URL..."
    spellcheck="false"
  />

  <div class="action-buttons">
    <div class="capture-wrapper">
      {#if pickMode !== 'none'}
        <button class="nav-btn cancel-btn" onclick={onCancelPick} title="Cancel (Esc)">
          <X size={14} />
        </button>
        <span class="pick-label">
          {pickMode === 'element' ? 'Select...' : 'Capture...'}
        </span>
      {:else}
        <button
          class="nav-btn"
          class:active={captureDropdownOpen}
          onclick={() => (captureDropdownOpen ? closeDropdown() : openDropdown())}
          title="Capture to Claude"
        >
          <Crosshair size={14} />
        </button>
      {/if}
      {#if captureDropdownOpen}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="capture-backdrop" onclick={closeDropdown}></div>
        <div class="capture-dropdown">
          <button
            class="capture-option"
            onclick={() => {
              closeDropdown()
              onStartElementPick()
            }}
          >
            <Crosshair size={13} />
            Select Element
          </button>
          <button
            class="capture-option"
            onclick={() => {
              closeDropdown()
              onStartRegionCapture()
            }}
          >
            <Camera size={13} />
            Capture Region
          </button>
        </div>
      {/if}
    </div>

    {#if isDevToolsOpen}
      <button
        class="nav-btn"
        onclick={onSwitchDevToolsMode}
        title="Switch DevTools position ({devToolsMode === 'bottom' ? 'right' : 'bottom'})"
      >
        {#if devToolsMode === 'bottom'}
          <PanelRight size={14} />
        {:else}
          <PanelBottom size={14} />
        {/if}
      </button>
    {/if}
    <button
      class="nav-btn"
      class:active={isDevToolsOpen}
      onclick={onToggleDevTools}
      title="Toggle DevTools"
    >
      <Code size={14} />
    </button>
  </div>
</div>

<style>
  .browser-toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 36px;
    padding: 0 6px;
    background: rgba(0, 0, 0, 0.3);
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    flex-shrink: 0;
  }

  .nav-buttons,
  .action-buttons {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  .nav-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    padding: 0;
  }

  .nav-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.9);
  }

  .nav-btn:disabled {
    color: rgba(255, 255, 255, 0.2);
    cursor: default;
  }

  .nav-btn.active {
    color: rgb(116, 192, 252);
    background: rgba(116, 192, 252, 0.12);
  }

  .nav-btn.loading {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .capture-wrapper {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .cancel-btn {
    color: rgb(255, 130, 130) !important;
  }

  .cancel-btn:hover {
    background: rgba(255, 130, 130, 0.12) !important;
  }

  .pick-label {
    font-size: 11px;
    color: rgba(116, 192, 252, 0.8);
    white-space: nowrap;
  }

  .capture-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }

  .capture-dropdown {
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 4px;
    min-width: 160px;
    padding: 4px;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
    z-index: 100;
  }

  .capture-option {
    display: flex;
    align-items: center;
    gap: 8px;
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
  }

  .capture-option:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .url-bar {
    flex: 1;
    height: 26px;
    padding: 0 8px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.3);
    color: rgba(255, 255, 255, 0.8);
    font-size: 12px;
    font-family: inherit;
    outline: none;
    min-width: 0;
  }

  .url-bar:focus {
    border-color: rgba(116, 192, 252, 0.5);
    color: rgba(255, 255, 255, 0.95);
  }

  .url-bar::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }
</style>
