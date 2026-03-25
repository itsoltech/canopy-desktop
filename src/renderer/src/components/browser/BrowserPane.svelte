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
  import {
    updateBrowserPaneUrl,
    getAiSessions,
    focusSessionByPtyId,
  } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import AiSessionPicker from './AiSessionPicker.svelte'
  import { showUrlToast } from '../../lib/stores/toast.svelte'

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
  let pickMode: 'none' | 'element' | 'region' = $state('none')
  let showPicker = $state(false)
  let pendingPayload: string | null = $state(null)
  let frozenScreenshot: string | null = $state(null)

  let session = $derived(browserSessions[browserId])
  let aiSessionCount = $derived(
    workspaceState.selectedWorktreePath
      ? getAiSessions(workspaceState.selectedWorktreePath).length
      : 0,
  )

  async function freeze(): Promise<void> {
    // Close DevTools before capture so screenshot matches full area
    if (session?.isDevToolsOpen) {
      window.api.toggleBrowserDevTools(browserId, session?.devToolsMode)
      await new Promise((r) => setTimeout(r, 150))
    }
    const dataUrl = await window.api.capturePageFull(browserId)
    if (dataUrl) {
      frozenScreenshot = dataUrl
      window.api.setBrowserVisible(browserId, false)
    }
  }

  function unfreeze(): void {
    frozenScreenshot = null
    if (active) {
      window.api.setBrowserVisible(browserId, true)
    }
  }

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

  // Escape to cancel pick/capture
  $effect(() => {
    if (pickMode === 'none') return
    const handler = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault()
        handleCancelPick()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  })

  // Freeze/unfreeze when global overlays (modals, palette) open/close
  $effect(() => {
    if (!active) return
    const onFreeze = (): void => {
      freeze()
    }
    const onUnfreeze = (): void => {
      unfreeze()
    }
    window.addEventListener('canopy:freeze-browsers', onFreeze)
    window.addEventListener('canopy:unfreeze-browsers', onUnfreeze)
    return () => {
      window.removeEventListener('canopy:freeze-browsers', onFreeze)
      window.removeEventListener('canopy:unfreeze-browsers', onUnfreeze)
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

  async function sendToClaude(payload: string): Promise<void> {
    const path = workspaceState.selectedWorktreePath
    if (!path) return

    const sessions = getAiSessions(path)
    if (sessions.length === 0) {
      showUrlToast('No AI sessions open')
      return
    }
    if (sessions.length === 1) {
      deliverToSession(sessions[0].sessionId, payload)
    } else {
      pendingPayload = payload
      window.dispatchEvent(new CustomEvent('canopy:freeze-browsers'))
      showPicker = true
    }
  }

  function deliverToSession(sessionId: string, payload: string): void {
    window.dispatchEvent(new CustomEvent('canopy:unfreeze-browsers'))
    window.api.writePty(sessionId, payload)
    focusSessionByPtyId(sessionId)
  }

  function handlePickerSelect(sessionId: string): void {
    showPicker = false
    if (pendingPayload) {
      deliverToSession(sessionId, pendingPayload)
      pendingPayload = null
    }
  }

  async function handleStartElementPick(): Promise<void> {
    pickMode = 'element'
    const html = await window.api.browserStartElementPick(browserId)
    pickMode = 'none'
    if (!html) return
    sendToClaude('```html\n' + html + '\n```\n')
  }

  async function handleStartRegionCapture(): Promise<void> {
    pickMode = 'region'
    const filePath = await window.api.browserStartRegionCapture(browserId)
    pickMode = 'none'
    if (!filePath) return
    sendToClaude(filePath + ' ')
  }

  function handleCancelPick(): void {
    window.api.browserCancelPick(browserId)
    pickMode = 'none'
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
      {pickMode}
      onNavigate={handleNavigate}
      onBack={handleBack}
      onForward={handleForward}
      onReload={handleReload}
      onToggleDevTools={handleToggleDevTools}
      onSwitchDevToolsMode={handleSwitchDevToolsMode}
      hasAiSessions={aiSessionCount > 0}
      onStartElementPick={handleStartElementPick}
      onStartRegionCapture={handleStartRegionCapture}
      onCancelPick={handleCancelPick}
    />
  {/if}

  <div class="browser-content" bind:this={containerEl}>
    {#if frozenScreenshot}
      <img class="frozen-screenshot" src={frozenScreenshot} alt="" />
    {/if}
    {#if session?.error}
      <BrowserError
        errorDescription={session.error.description}
        validatedURL={session.error.url}
        onRetry={handleReload}
      />
    {/if}
  </div>
</div>

{#if showPicker}
  {@const path = workspaceState.selectedWorktreePath}
  {#if path}
    <AiSessionPicker
      sessions={getAiSessions(path)}
      onSelect={handlePickerSelect}
      onClose={() => {
        showPicker = false
        pendingPayload = null
        window.dispatchEvent(new CustomEvent('canopy:unfreeze-browsers'))
      }}
    />
  {/if}
{/if}

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

  .frozen-screenshot {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: top left;
    z-index: 1;
  }
</style>
