<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    RotateCw,
    Code,
    PanelBottom,
    PanelLeft,
    Crosshair,
    Camera,
    X,
    Smartphone,
    Star,
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
    activeDevice = null,
    viewports = {},
    onNavigate,
    onBack,
    onForward,
    onReload,
    onToggleDevTools,
    onSwitchDevToolsMode,
    onStartElementPick,
    onStartRegionCapture,
    onCancelPick,
    onSetDevice,
    isFavorited = false,
    onToggleFavorite,
  }: {
    url: string
    canGoBack: boolean
    canGoForward: boolean
    isLoading: boolean
    isDevToolsOpen: boolean
    devToolsMode: 'bottom' | 'left'
    pickMode?: 'none' | 'element' | 'region'
    hasAiSessions?: boolean
    activeDevice?: string | null
    viewports?: Record<
      string,
      { width: number; height: number; scaleFactor: number; mobile: boolean }
    >
    onNavigate: (url: string) => void
    onBack: () => void
    onForward: () => void
    onReload: () => void
    onToggleDevTools: () => void
    onSwitchDevToolsMode: () => void
    onStartElementPick: () => void
    onStartRegionCapture: () => void
    onCancelPick: () => void
    onSetDevice: (name: string | null) => void
    isFavorited?: boolean
    onToggleFavorite: () => void
  } = $props()

  let captureDropdownOpen = $state(false)
  let deviceDropdownOpen = $state(false)

  function openDropdown(): void {
    captureDropdownOpen = true
  }

  function closeDropdown(): void {
    captureDropdownOpen = false
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

<div class="flex items-center gap-1 h-9 px-1.5 bg-bg-input border-b border-active flex-shrink-0">
  <div class="flex items-center gap-0.5 flex-shrink-0">
    <button
      class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent text-text-secondary cursor-pointer p-0 enabled:hover:bg-active enabled:hover:text-text disabled:text-text-faint disabled:cursor-default"
      disabled={!canGoBack}
      onclick={onBack}
      title="Back"
      aria-label="Go back"
    >
      <ArrowLeft size={14} />
    </button>
    <button
      class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent text-text-secondary cursor-pointer p-0 enabled:hover:bg-active enabled:hover:text-text disabled:text-text-faint disabled:cursor-default"
      disabled={!canGoForward}
      onclick={onForward}
      title="Forward"
      aria-label="Go forward"
    >
      <ArrowRight size={14} />
    </button>
    <button
      class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent text-text-secondary cursor-pointer p-0 hover:bg-active hover:text-text"
      onclick={onReload}
      title="Reload"
      aria-label="Reload"
    >
      <RotateCw size={14} class={isLoading ? 'animate-spin-slow motion-reduce:animate-none' : ''} />
    </button>
  </div>

  <input
    bind:this={urlInput}
    class="flex-1 h-6.5 px-2 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none min-w-0 focus:border-focus-ring placeholder:text-text-faint"
    type="text"
    bind:value={inputValue}
    onkeydown={handleKeydown}
    onfocus={handleFocus}
    placeholder="Enter URL..."
    spellcheck="false"
  />

  {#if url && url !== 'about:blank'}
    <button
      class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent cursor-pointer p-0 hover:bg-active"
      class:text-warning={isFavorited}
      class:text-text-secondary={!isFavorited}
      class:hover:text-text={!isFavorited}
      onclick={onToggleFavorite}
      title={isFavorited ? 'Remove from favorites' : 'Add to favorites'}
      aria-label={isFavorited ? 'Remove from favorites' : 'Add to favorites'}
    >
      <Star size={14} fill={isFavorited ? 'currentColor' : 'none'} />
    </button>
  {/if}

  <div class="flex items-center gap-0.5 flex-shrink-0">
    <div class="relative flex items-center gap-1">
      {#if pickMode !== 'none'}
        <button
          class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent text-danger-text cursor-pointer p-0 hover:bg-danger-bg"
          onclick={onCancelPick}
          title="Cancel (Esc)"
          aria-label="Cancel pick"
        >
          <X size={14} />
        </button>
        <span class="text-xs text-accent-text whitespace-nowrap">
          {pickMode === 'element' ? 'Select...' : 'Capture...'}
        </span>
      {:else}
        <button
          class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent cursor-pointer p-0 enabled:hover:bg-active enabled:hover:text-text disabled:text-text-faint disabled:cursor-default"
          class:text-accent={captureDropdownOpen}
          class:bg-accent-bg={captureDropdownOpen}
          class:text-text-secondary={!captureDropdownOpen}
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
        <div class="fixed inset-0 z-overlay" onclick={closeDropdown}></div>
        <div
          class="absolute top-full right-0 mt-1 min-w-40 p-1 bg-bg-overlay border border-border rounded-lg shadow-popover z-popover"
        >
          <button
            class="flex items-center gap-2 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left hover:bg-active"
            onclick={() => {
              closeDropdown()
              onStartElementPick()
            }}
          >
            <Crosshair size={13} />
            Select Element
          </button>
          <button
            class="flex items-center gap-2 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-text text-sm font-inherit cursor-pointer text-left hover:bg-active"
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

    <div class="relative flex items-center gap-1">
      <button
        class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent cursor-pointer p-0 hover:bg-active hover:text-text"
        class:text-accent={activeDevice !== null || deviceDropdownOpen}
        class:bg-accent-bg={activeDevice !== null || deviceDropdownOpen}
        class:text-text-secondary={!(activeDevice !== null || deviceDropdownOpen)}
        onclick={() => (deviceDropdownOpen = !deviceDropdownOpen)}
        title="Responsive mode"
        aria-label="Responsive mode"
      >
        <Smartphone size={14} />
      </button>
      {#if deviceDropdownOpen}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="fixed inset-0 z-overlay" onclick={() => (deviceDropdownOpen = false)}></div>
        <div
          class="absolute top-full right-0 mt-1 min-w-40 p-1 bg-bg-overlay border border-border rounded-lg shadow-popover z-popover"
        >
          {#if activeDevice}
            <button
              class="flex items-center gap-2 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-danger-text text-sm font-inherit cursor-pointer text-left hover:bg-active"
              onclick={() => {
                deviceDropdownOpen = false
                onSetDevice(null)
              }}
            >
              <X size={13} />
              Reset to Desktop
            </button>
            <div class="h-px mx-1.5 my-0.5 bg-border-subtle"></div>
          {/if}
          {#each Object.entries(viewports) as [name, preset] (name)}
            <button
              class="flex items-center gap-2 w-full px-2.5 py-1.5 border-0 rounded-md bg-transparent text-sm font-inherit cursor-pointer text-left hover:bg-active"
              class:text-accent={activeDevice === name}
              class:text-text={activeDevice !== name}
              onclick={() => {
                deviceDropdownOpen = false
                onSetDevice(name)
              }}
            >
              <Smartphone size={13} />
              {name}
              <span class="ml-auto text-2xs text-text-muted"
                >{preset.width}&times;{preset.height}</span
              >
            </button>
          {/each}
        </div>
      {/if}
    </div>

    {#if isDevToolsOpen}
      <button
        class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent text-text-secondary cursor-pointer p-0 hover:bg-active hover:text-text"
        onclick={onSwitchDevToolsMode}
        title="Switch DevTools position ({devToolsMode === 'bottom' ? 'left' : 'bottom'})"
        aria-label="Switch DevTools position to {devToolsMode === 'bottom' ? 'left' : 'bottom'}"
      >
        {#if devToolsMode === 'bottom'}
          <PanelLeft size={14} />
        {:else}
          <PanelBottom size={14} />
        {/if}
      </button>
    {/if}
    <button
      class="flex items-center justify-center w-6.5 h-6.5 border-0 rounded-md bg-transparent cursor-pointer p-0 hover:bg-active hover:text-text"
      class:text-accent={isDevToolsOpen}
      class:bg-accent-bg={isDevToolsOpen}
      class:text-text-secondary={!isDevToolsOpen}
      onclick={onToggleDevTools}
      title="Toggle DevTools"
      aria-label="Toggle DevTools"
    >
      <Code size={14} />
    </button>
  </div>
</div>
