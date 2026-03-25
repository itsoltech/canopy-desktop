<script lang="ts">
  import { ArrowLeft, ArrowRight, RotateCw, Code, PanelBottom, PanelRight } from 'lucide-svelte'

  let {
    url,
    canGoBack,
    canGoForward,
    isLoading,
    isDevToolsOpen,
    devToolsMode,
    onNavigate,
    onBack,
    onForward,
    onReload,
    onToggleDevTools,
    onSwitchDevToolsMode,
  }: {
    url: string
    canGoBack: boolean
    canGoForward: boolean
    isLoading: boolean
    isDevToolsOpen: boolean
    devToolsMode: 'bottom' | 'right'
    onNavigate: (url: string) => void
    onBack: () => void
    onForward: () => void
    onReload: () => void
    onToggleDevTools: () => void
    onSwitchDevToolsMode: () => void
  } = $props()

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
