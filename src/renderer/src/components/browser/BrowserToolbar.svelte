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
    hasAiSessions = false,
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
    hasAiSessions?: boolean
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

  let inputValue = $state('')
  let urlInput: HTMLInputElement | undefined = $state()

  // Sync input value when url prop changes (navigation)
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
    <button
      class="nav-btn"
      disabled={!canGoBack}
      onclick={onBack}
      title="Back"
      aria-label="Go back"
    >
      <ArrowLeft size={14} />
    </button>
    <button
      class="nav-btn"
      disabled={!canGoForward}
      onclick={onForward}
      title="Forward"
      aria-label="Go forward"
    >
      <ArrowRight size={14} />
    </button>
    <button
      class="nav-btn"
      class:loading={isLoading}
      onclick={onReload}
      title="Reload"
      aria-label="Reload"
    >
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
        <button
          class="nav-btn cancel-btn"
          onclick={onCancelPick}
          title="Cancel (Esc)"
          aria-label="Cancel pick"
        >
          <X size={14} />
        </button>
        <span class="pick-label">
          {pickMode === 'element' ? 'Select...' : 'Capture...'}
        </span>
      {:else}
        <button
          class="nav-btn"
          class:active={captureDropdownOpen}
          disabled={!hasAiSessions}
          onclick={() => (captureDropdownOpen ? closeDropdown() : openDropdown())}
          title={hasAiSessions ? 'Capture to AI agent' : 'No AI sessions open'}
          aria-label={hasAiSessions ? 'Capture to AI agent' : 'No AI sessions open'}
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
        aria-label="Switch DevTools position to {devToolsMode === 'bottom' ? 'right' : 'bottom'}"
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
      aria-label="Toggle DevTools"
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
    background: var(--c-bg-input);
    border-bottom: 1px solid var(--c-active);
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
    color: var(--c-text-secondary);
    cursor: pointer;
    padding: 0;
  }

  .nav-btn:hover:not(:disabled) {
    background: var(--c-active);
    color: var(--c-text);
  }

  .nav-btn:disabled {
    color: var(--c-text-faint);
    cursor: default;
  }

  .nav-btn.active {
    color: var(--c-accent);
    background: var(--c-accent-bg);
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

  @media (prefers-reduced-motion: reduce) {
    .nav-btn.loading {
      animation: none;
    }
  }

  .capture-wrapper {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .cancel-btn {
    color: var(--c-danger-text) !important;
  }

  .cancel-btn:hover {
    background: var(--c-danger-bg) !important;
  }

  .pick-label {
    font-size: 11px;
    color: var(--c-accent-text);
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
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
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
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }

  .capture-option:hover {
    background: var(--c-active);
  }

  .url-bar {
    flex: 1;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
    min-width: 0;
  }

  .url-bar:focus {
    border-color: var(--c-focus-ring);
    color: var(--c-text);
  }

  .url-bar::placeholder {
    color: var(--c-text-faint);
  }
</style>
