<script lang="ts">
  import { onMount } from 'svelte'
  import BrowserToolbar from './BrowserToolbar.svelte'
  import BrowserError from './BrowserError.svelte'
  import {
    browserSessions,
    initBrowserSession,
    removeBrowserSession,
    handleBrowserUrlChanged,
    handleBrowserTitleChanged,
    handleBrowserLoadingChanged,
    handleBrowserFaviconChanged,
    handleBrowserLoadFailed,
    handleBrowserStateChanged,
  } from '../../lib/browser/browserState.svelte'
  import { updateBrowserPaneUrl } from '../../lib/stores/tabs.svelte'

  let {
    browserId,
    active,
    onTitleChange,
  }: {
    browserId: string
    active: boolean
    onTitleChange: (title: string) => void
  } = $props()

  let containerEl: HTMLDivElement | undefined = $state()
  let toolbarComponent: BrowserToolbar | undefined = $state()

  let session = $derived(browserSessions[browserId])

  // Subscribe to browser events
  onMount(() => {
    initBrowserSession(browserId)

    const unsubs = [
      window.api.onBrowserUrlChanged((data) => {
        if (data.browserId === browserId) {
          handleBrowserUrlChanged(browserId, data.url)
          updateBrowserPaneUrl(browserId, data.url)
        }
      }),
      window.api.onBrowserTitleChanged((data) => {
        if (data.browserId === browserId) {
          handleBrowserTitleChanged(browserId, data.title)
          onTitleChange(data.title)
        }
      }),
      window.api.onBrowserFaviconChanged((data) => {
        if (data.browserId === browserId) {
          handleBrowserFaviconChanged(browserId, data.favicon)
        }
      }),
      window.api.onBrowserLoadingChanged((data) => {
        if (data.browserId === browserId) {
          handleBrowserLoadingChanged(browserId, data.isLoading)
        }
      }),
      window.api.onBrowserLoadFailed((data) => {
        if (data.browserId === browserId) {
          handleBrowserLoadFailed(
            browserId,
            data.errorCode,
            data.errorDescription,
            data.validatedURL,
          )
        }
      }),
      window.api.onBrowserStateChanged((data) => {
        if (data.browserId === browserId) {
          handleBrowserStateChanged(browserId, data)
        }
      }),
    ]

    return () => {
      unsubs.forEach((fn) => fn())
      removeBrowserSession(browserId)
    }
  })

  // Listen for Cmd+L focus event
  $effect(() => {
    if (!active) return
    const handler = (): void => toolbarComponent?.focusUrlBar()
    window.addEventListener('canopy:focus-url-bar', handler)
    return () => window.removeEventListener('canopy:focus-url-bar', handler)
  })

  // Track bounds with ResizeObserver and send to main process
  $effect(() => {
    if (!containerEl) return

    function sendBounds(): void {
      if (!containerEl) return
      const rect = containerEl.getBoundingClientRect()
      if (rect.width <= 0 || rect.height <= 0) return
      window.api.setBrowserBounds(browserId, {
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      })
    }

    const observer = new ResizeObserver(() => sendBounds())
    observer.observe(containerEl)
    // Initial bounds
    sendBounds()

    return () => observer.disconnect()
  })

  // Show/hide based on active state
  $effect(() => {
    window.api.setBrowserVisible(browserId, active)
  })

  function handleNavigate(url: string): void {
    window.api.navigateBrowser(browserId, url)
  }

  function handleBack(): void {
    window.api.browserBack(browserId)
  }

  function handleForward(): void {
    window.api.browserForward(browserId)
  }

  function handleReload(): void {
    window.api.browserReload(browserId)
  }

  function handleToggleDevTools(): void {
    window.api.toggleBrowserDevTools(browserId, session?.devToolsMode)
  }

  function handleSwitchDevToolsMode(): void {
    const newMode = session?.devToolsMode === 'bottom' ? 'right' : 'bottom'
    // Close and reopen with new mode
    window.api.toggleBrowserDevTools(browserId, session?.devToolsMode)
    setTimeout(() => {
      window.api.toggleBrowserDevTools(browserId, newMode)
    }, 100)
  }

  export function focusUrlBar(): void {
    toolbarComponent?.focusUrlBar()
  }
</script>

<div class="browser-pane">
  {#if session}
    <BrowserToolbar
      bind:this={toolbarComponent}
      url={session.url}
      canGoBack={session.canGoBack}
      canGoForward={session.canGoForward}
      isLoading={session.isLoading}
      isDevToolsOpen={session.isDevToolsOpen}
      devToolsMode={session.devToolsMode}
      onNavigate={handleNavigate}
      onBack={handleBack}
      onForward={handleForward}
      onReload={handleReload}
      onToggleDevTools={handleToggleDevTools}
      onSwitchDevToolsMode={handleSwitchDevToolsMode}
    />
  {/if}

  <div class="browser-content" bind:this={containerEl}>
    {#if session?.error}
      <BrowserError
        errorDescription={session.error.description}
        validatedURL={session.error.url}
        onRetry={handleReload}
      />
    {/if}
  </div>
</div>

<style>
  .browser-pane {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .browser-content {
    flex: 1;
    position: relative;
    min-height: 0;
  }
</style>
