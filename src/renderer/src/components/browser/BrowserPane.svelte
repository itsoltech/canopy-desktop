<script lang="ts">
  import { match, P } from 'ts-pattern'
  import { onMount, untrack } from 'svelte'
  import BrowserToolbar from './BrowserToolbar.svelte'
  import BrowserError from './BrowserError.svelte'
  import {
    browserSessions,
    initBrowserSession,
    handleBrowserUrlChanged,
    handleBrowserTitleChanged,
    handleBrowserLoadingChanged,
    handleBrowserFaviconChanged,
    handleBrowserLoadFailed,
    handleBrowserStateChanged,
    getAllViewports,
    isFavorite,
    addFavorite,
    removeFavorite,
    removeFavoritesByHost,
    updateFavorite,
    reorderFavorites,
    getFavorites,
  } from '../../lib/browser/browserState.svelte'
  import {
    updateBrowserPaneUrl,
    getAiSessions,
    focusSessionByPtyId,
    openTool,
  } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import AiSessionPicker from './AiSessionPicker.svelte'
  import { showUrlToast } from '../../lib/stores/toast.svelte'
  import { prefs } from '../../lib/stores/preferences.svelte'
  import { dragState } from '../../lib/stores/dragState.svelte'
  import type { WebviewElement } from '../../lib/browser/browserState.svelte'

  let {
    browserId,
    worktreePath,
    active,
    focused,
    initialUrl,
    onTitleChange,
    onFocus,
  }: {
    browserId: string
    worktreePath: string
    active: boolean
    focused?: boolean
    initialUrl?: string
    onTitleChange: (title: string) => void
    onFocus?: () => void
  } = $props()

  let webviewEl: HTMLElement | undefined = $state()
  let devtoolsPlaceholder: HTMLDivElement | undefined = $state()
  let contentEl: HTMLDivElement | undefined = $state()
  let toolbarComponent: BrowserToolbar | undefined = $state()
  let pickMode: 'none' | 'element' | 'region' = $state('none')
  let showPicker = $state(false)
  let pendingPayload: string | null = $state(null)
  let devtoolsOpen = $state(false)
  let devtoolsMode: 'bottom' | 'left' = $state('bottom')
  let devtoolsRatio = $state(0.6) // browser gets 60%, devtools gets 40%
  let dividerDragging = $state(false)
  let activeDevice: string | null = $state(null)
  let appOverlayOpen = $state(false)
  let wrapperEl: HTMLDivElement | undefined = $state()
  let wrapperSize = $state({ w: 0, h: 0 })
  let backgroundThrottlingInitialized = false

  // Compute scale factor so device frame fits in wrapper while preserving aspect ratio.
  // The webview always renders at full device resolution; CSS transform scales it down.
  let deviceScale = $derived.by(() => {
    if (!activeDevice || !viewports[activeDevice]) return 1
    const d = viewports[activeDevice]
    const labelH = 22
    const availW = wrapperSize.w
    const availH = wrapperSize.h - labelH
    if (availW <= 0 || availH <= 0) return 0
    return Math.min(1, availW / d.width, availH / d.height)
  })

  let alive = true
  let registeredElement: HTMLElement | null = null

  let session = $derived(browserSessions[browserId])
  let aiSessionCount = $derived(
    workspaceState.selectedWorktreePath
      ? getAiSessions(workspaceState.selectedWorktreePath).length
      : 0,
  )
  let viewports = $derived(getAllViewports())

  function wv(): WebviewElement | null {
    return (webviewEl as WebviewElement) ?? null
  }

  // --- Navigation ---

  function addScheme(url: string): string {
    if (/^https?:\/\//i.test(url)) return url
    const isLocal = /^(localhost|127\.0\.0\.1)(:\d+)?(\/|$)/i.test(url)
    return (isLocal ? 'http://' : 'https://') + url
  }

  function handleNavigate(url: string): void {
    const w = wv()
    if (!w) return
    const finalUrl = addScheme(url)
    try {
      const parsed = new URL(finalUrl)
      if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return
    } catch {
      return
    }
    w.loadURL(finalUrl)
  }

  function handleBack(): void {
    const w = wv()
    if (w?.canGoBack()) w.goBack()
  }

  function handleForward(): void {
    const w = wv()
    if (w?.canGoForward()) w.goForward()
  }

  function handleReload(): void {
    wv()?.reload()
  }

  // --- Device emulation ---

  function handleSetDevice(name: string | null): void {
    activeDevice = name
    const device = name ? (viewports[name] ?? null) : null
    window.api.setBrowserDeviceEmulation(browserId, device)
  }

  /** Watch for password fields added dynamically (SPA forms) */
  function injectPasswordFieldObserver(): void {
    const w = wv()
    if (!w) return
    w.executeJavaScript(
      `
      (function() {
        if (window.__canopyPwObserverCleanup) {
          window.__canopyPwObserverCleanup()
        }

        const cleanup = () => {
          obs.disconnect()
          clearTimeout(timeoutId)
          delete window.__canopyPwObserverCleanup
        }

        const obs = new MutationObserver(() => {
          const pw = document.querySelector('input[type="password"]')
          if (pw && !document.getElementById('__canopy_autofill_icon')) {
            cleanup()
            console.log('__CANOPY_PW_FIELD_FOUND__')
          }
        })
        obs.observe(document.body, { childList: true, subtree: true })
        const timeoutId = setTimeout(cleanup, 10000)
        window.__canopyPwObserverCleanup = cleanup
      })()
    `,
    ).catch(() => {})
  }

  // --- Mouse back/forward navigation ---
  // app-command is Windows-only; on macOS we inject a mouseup listener
  // inside the webview and communicate via console.log marker.

  function injectMouseNavigation(): void {
    const w = wv()
    if (!w) return
    w.executeJavaScript(
      `
      (function() {
        if (window.__canopyMouseNavInstalled) return
        window.__canopyMouseNavInstalled = true
        document.addEventListener('mouseup', (e) => {
          if (e.button === 3) { console.log('__CANOPY_NAV__:back') }
          if (e.button === 4) { console.log('__CANOPY_NAV__:forward') }
        }, true)
      })()
    `,
    ).catch(() => {})
  }

  // --- Favorites ---

  let favModalOpen = $state(false)
  let favModalMode: 'add' | 'edit' = $state('add')
  let favEditOriginalUrl = $state('')
  let favName = $state('')
  let favUrl = $state('')
  let favCtxMenu: { x: number; y: number; index: number } | null = $state(null)
  let favDragIndex: number | null = $state(null)
  let favDropIndex: number | null = $state(null)
  let starDropdownOpen = $state(false)

  function handleToggleFavorite(): void {
    const url = session?.url
    if (!url || url === 'about:blank') return
    if (isFavorite(url)) {
      // Already favorited — show dropdown with options
      starDropdownOpen = !starDropdownOpen
    } else {
      favName = session?.title || new URL(url).hostname
      favUrl = url
      favModalMode = 'add'
      favModalOpen = true
    }
  }

  function handleStarRemoveAll(): void {
    const url = session?.url
    if (url) removeFavoritesByHost(url)
    starDropdownOpen = false
  }

  function handleStarAddNew(): void {
    const url = session?.url
    if (!url) return
    favName = session?.title || new URL(url).hostname
    favUrl = url
    favModalMode = 'add'
    favModalOpen = true
    starDropdownOpen = false
  }

  function handleFavModalSave(): void {
    const name = favName.trim()
    const url = favUrl.trim()
    if (!name || !url) return
    if (favModalMode === 'edit') {
      updateFavorite(favEditOriginalUrl, {
        url,
        name,
        favicon: getFavorites().find((f) => f.url === favEditOriginalUrl)?.favicon ?? null,
      })
    } else {
      addFavorite({ url, name, favicon: session?.favicon ?? null })
    }
    favModalOpen = false
  }

  function handleFavContextMenu(e: MouseEvent, index: number): void {
    e.preventDefault()
    favCtxMenu = { x: e.clientX, y: e.clientY, index }
  }

  function handleFavEdit(index: number): void {
    const fav = getFavorites()[index]
    if (!fav) return
    favEditOriginalUrl = fav.url
    favName = fav.name
    favUrl = fav.url
    favModalMode = 'edit'
    favModalOpen = true
    favCtxMenu = null
  }

  function handleFavDuplicate(index: number): void {
    const fav = getFavorites()[index]
    if (!fav) return
    favName = fav.name + ' (copy)'
    favUrl = session?.url || fav.url
    favModalMode = 'add'
    favModalOpen = true
    favCtxMenu = null
  }

  function handleFavDelete(index: number): void {
    const fav = getFavorites()[index]
    if (fav) removeFavorite(fav.url)
    favCtxMenu = null
  }

  function handleFavDragStart(e: DragEvent, index: number): void {
    favDragIndex = index
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
  }

  function handleFavDragOver(e: DragEvent, index: number): void {
    e.preventDefault()
    favDropIndex = index
  }

  function handleFavDrop(e: DragEvent, index: number): void {
    e.preventDefault()
    if (favDragIndex !== null && favDragIndex !== index) {
      reorderFavorites(favDragIndex, index)
    }
    favDragIndex = null
    favDropIndex = null
  }

  // --- Credentials ---

  let hasCredentials = $state(false)
  let savePrompt: { domain: string; username: string; password: string; title: string } | null =
    $state(null)
  let pageHasPasswordField = $state(false)
  let lastCapturedCreds: {
    domain: string
    username: string
    password: string
    title: string
  } | null = null
  let capturedCredsTimer: ReturnType<typeof setTimeout> | null = null

  function setLastCapturedCreds(
    data: { domain: string; username: string; password: string; title: string } | null,
  ): void {
    lastCapturedCreds = data
    if (capturedCredsTimer) clearTimeout(capturedCredsTimer)
    if (data) {
      // Auto-clear password from memory after 30s
      capturedCredsTimer = setTimeout(() => {
        lastCapturedCreds = null
      }, 30_000)
    }
  }

  async function checkCredentials(): Promise<void> {
    const w = wv()
    if (!w) return
    const url = w.getURL()
    if (!url || url === 'about:blank') {
      hasCredentials = false
      pageHasPasswordField = false
      return
    }
    try {
      const domain = new URL(url).host
      const creds = await window.api.getCredentials(domain)
      hasCredentials = creds.length > 0

      // Detect password fields (now + watch for dynamically added ones)
      const result = await w.executeJavaScript(`
        !!document.querySelector('input[type="password"]')
      `)
      pageHasPasswordField = !!result

      if (pageHasPasswordField) {
        injectCredentialCapture()
        if (hasCredentials) injectAutofillIcon(creds)
      }

      // Watch for dynamically added password fields (SPA login forms)
      if (hasCredentials) {
        injectPasswordFieldObserver()
      }
    } catch {
      hasCredentials = false
      pageHasPasswordField = false
    }
  }

  function injectAutofillIcon(
    creds: Array<{ id: string; username: string; title?: string }>,
  ): void {
    const w = wv()
    if (!w) return

    const credsJson = JSON.stringify(
      creds.map((c) => ({ id: c.id, u: c.username, t: c.title || '' })),
    )

    w.executeJavaScript(
      `
      (function() {
        if (document.getElementById('__canopy_autofill_icon')) return
        const pw = document.querySelector('input[type="password"]')
        if (!pw) return
        const creds = ${credsJson}

        const svgKey = '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/><circle cx="16.5" cy="7.5" r=".5" fill="currentColor"/></svg>'

        const icon = document.createElement('div')
        icon.id = '__canopy_autofill_icon'
        icon.innerHTML = svgKey
        icon.title = 'Autofill saved password'
        icon.style.cssText = 'position:absolute;right:6px;top:50%;transform:translateY(-50%);cursor:pointer;z-index:99999;color:#4a9eed;display:flex;align-items:center;justify-content:center;width:24px;height:24px;border-radius:6px;background:color-mix(in srgb,currentColor 8%,transparent);backdrop-filter:blur(6px);box-shadow:0 0 0 1px rgba(128,128,128,0.2);transition:all 0.15s'
        icon.onmouseenter = () => { icon.style.background = 'color-mix(in srgb,currentColor 15%,transparent)'; icon.style.boxShadow = '0 0 0 1px rgba(74,158,237,0.4)' }
        icon.onmouseleave = () => { icon.style.background = 'color-mix(in srgb,currentColor 8%,transparent)'; icon.style.boxShadow = '0 0 0 1px rgba(128,128,128,0.2)' }

        icon.onclick = (e) => {
          e.preventDefault(); e.stopPropagation()
          let existing = document.getElementById('__canopy_cred_picker')
          if (existing) { existing.remove(); return }

          const picker = document.createElement('div')
          picker.id = '__canopy_cred_picker'
          picker.style.cssText = 'position:absolute;right:0;top:100%;margin-top:4px;min-width:200px;padding:4px;background:rgba(30,30,30,0.98);border:1px solid rgba(255,255,255,0.12);border-radius:8px;box-shadow:0 8px 24px rgba(0,0,0,0.6);z-index:100000;backdrop-filter:blur(8px)'

          const header = document.createElement('div')
          header.textContent = 'Saved accounts'
          header.style.cssText = 'padding:6px 10px 4px;font-size:10px;color:rgba(255,255,255,0.35);text-transform:uppercase;letter-spacing:0.5px'
          picker.appendChild(header)

          for (const c of creds) {
            const btn = document.createElement('button')
            btn.style.cssText = 'display:flex;align-items:center;gap:8px;width:100%;padding:8px 10px;border:none;border-radius:6px;background:none;color:rgba(255,255,255,0.85);font-size:12px;cursor:pointer;text-align:left;font-family:inherit;transition:background 0.1s'
            const avatar = document.createElement('div')
            avatar.textContent = (c.u || '?')[0].toUpperCase()
            avatar.style.cssText = 'width:24px;height:24px;border-radius:50%;background:rgba(116,192,252,0.15);color:rgba(116,192,252,0.8);display:flex;align-items:center;justify-content:center;font-size:11px;font-weight:600;flex-shrink:0'
            const info = document.createElement('div')
            info.style.cssText = 'overflow:hidden'
            const name = document.createElement('div')
            name.textContent = c.u || '(no username)'
            name.style.cssText = 'font-size:12px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis'
            if (c.t) {
              const titleSpan = document.createElement('div')
              titleSpan.textContent = c.t
              titleSpan.style.cssText = 'font-size:10px;color:rgba(255,255,255,0.3);white-space:nowrap;overflow:hidden;text-overflow:ellipsis'
              info.appendChild(name)
              info.appendChild(titleSpan)
            } else {
              info.appendChild(name)
            }
            const dots = document.createElement('div')
            dots.textContent = '\\u2022\\u2022\\u2022\\u2022\\u2022\\u2022\\u2022\\u2022'
            dots.style.cssText = 'font-size:10px;color:rgba(255,255,255,0.3);letter-spacing:1px'
            info.appendChild(dots)
            btn.appendChild(avatar)
            btn.appendChild(info)
            btn.onmouseenter = () => { btn.style.background = 'rgba(255,255,255,0.06)' }
            btn.onmouseleave = () => { btn.style.background = 'none' }
            btn.onclick = (ev) => {
              ev.preventDefault(); ev.stopPropagation()
              console.log('__CANOPY_FILL__:' + c.id)
              picker.remove()
            }
            picker.appendChild(btn)
          }

          icon.parentElement.appendChild(picker)
          const closePicker = (ev) => {
            if (!picker.contains(ev.target) && ev.target !== icon) {
              picker.remove()
              document.removeEventListener('click', closePicker)
            }
          }
          setTimeout(() => document.addEventListener('click', closePicker), 0)
        }

        // Wrap just the input in a positioning container
        const wrap = document.createElement('div')
        wrap.id = '__canopy_autofill_wrap'
        wrap.style.cssText = 'position:relative;display:inline-block;width:' + pw.offsetWidth + 'px'
        pw.parentElement.insertBefore(wrap, pw)
        wrap.appendChild(pw)
        wrap.appendChild(icon)
      })()
    `,
    ).catch(() => {})
  }

  async function handleAutofillCredential(credId: string): Promise<void> {
    const w = wv()
    if (!w) return
    const url = w.getURL()
    if (!url) return
    const domain = new URL(url).host
    const cred = await window.api.getCredentialDecrypted(credId, domain, 'autofill')
    if (!cred) return
    // Fill via main process isolated world — page scripts cannot intercept
    await window.api.fillBrowserCredential(browserId, cred.username, cred.password)
  }

  /** Inject early capture script — stores credentials on form submit/button click */
  function injectCredentialCapture(): void {
    const w = wv()
    if (!w) return
    w.executeJavaScript(
      `
      (function() {
        if (window.__canopyCaptureInstalled) return
        window.__canopyCaptureInstalled = true
        window.__canopyCapturedCreds = null

        function capture() {
          const pw = document.querySelector('input[type="password"]')
          if (!pw || !pw.value) return
          console.log('__CANOPY_CREDS_READY__')
        }

        // Capture on form submit
        document.addEventListener('submit', () => capture(), true)

        // Capture on click anywhere (covers <span onclick>, <button>, etc.)
        document.addEventListener('click', (e) => {
          const pw = document.querySelector('input[type="password"]')
          if (!pw || !pw.value) return
          // Only capture if click target is near a submit-like element
          const target = e.target
          const isSubmit = target.closest('[type="submit"],button,a,.submit,[role="button"]')
          if (isSubmit) capture()
        }, true)

        // Capture on Enter key in password field
        document.addEventListener('keydown', (e) => {
          if (e.key === 'Enter') {
            const pw = document.querySelector('input[type="password"]')
            if (pw && pw.value) capture()
          }
        }, true)
      })()
    `,
    ).catch(() => {})
  }

  /** Check renderer-stored captured creds after navigation */
  async function checkCapturedCredentials(): Promise<void> {
    const captured = lastCapturedCreds
    setLastCapturedCreds(null)
    if (!captured || !captured.password || !captured.domain) return
    try {
      const existing = await window.api.getCredentials(captured.domain)
      const alreadySaved = existing.some(
        (c: { username: string }) => c.username === captured.username,
      )
      if (!alreadySaved) {
        savePrompt = captured
      }
    } catch {
      // ignore
    }
  }

  function handleSaveCredential(): void {
    if (!savePrompt) return
    window.api.saveCredential(
      savePrompt.domain,
      savePrompt.username,
      savePrompt.password,
      savePrompt.title,
    )
    savePrompt = null
    checkCredentials()
  }

  function handleDismissSavePrompt(): void {
    savePrompt = null
  }

  // --- DevTools (WebContentsView managed by main process) ---

  function handleToggleDevTools(): void {
    if (devtoolsOpen) {
      window.api.closeBrowserDevTools(browserId)
      devtoolsOpen = false
      updateDevToolsState(false)
    } else {
      window.api.openBrowserDevTools(browserId)
      devtoolsOpen = true
      updateDevToolsState(true)
    }
  }

  function handleSwitchDevToolsMode(): void {
    devtoolsMode = devtoolsMode === 'bottom' ? 'left' : 'bottom'
    updateDevToolsState(true)
    requestAnimationFrame(() => sendDevToolsBounds())
  }

  function updateDevToolsState(isOpen: boolean): void {
    handleBrowserStateChanged(browserId, {
      canGoBack: wv()?.canGoBack() ?? false,
      canGoForward: wv()?.canGoForward() ?? false,
      isDevToolsOpen: isOpen,
      devToolsMode: devtoolsMode,
    })
  }

  // Track wrapper size for device frame scaling
  $effect(() => {
    if (!wrapperEl) return
    const observer = new ResizeObserver(([entry]) => {
      wrapperSize = { w: entry.contentRect.width, h: entry.contentRect.height }
    })
    observer.observe(wrapperEl)
    return () => observer.disconnect()
  })

  $effect(() => {
    const nextActive = active
    const stableBrowserId = untrack(() => browserId)

    if (!backgroundThrottlingInitialized) {
      backgroundThrottlingInitialized = true
      if (nextActive) return
    }

    window.api.setBrowserBackgroundThrottling(stableBrowserId, !nextActive).catch(() => {})
  })

  // --- DevTools divider drag (pointer capture pattern from SplitDivider) ---

  let dragStartPos = 0

  function handleDividerPointerDown(e: PointerEvent): void {
    e.preventDefault()
    const target = e.currentTarget as HTMLElement
    target.setPointerCapture(e.pointerId)
    dividerDragging = true
    dragStartPos = devtoolsMode === 'left' ? e.clientX : e.clientY
  }

  function handleDividerPointerMove(e: PointerEvent): void {
    if (!dividerDragging || !contentEl) return
    const currentPos = devtoolsMode === 'left' ? e.clientX : e.clientY
    const delta = currentPos - dragStartPos
    if (delta === 0) return
    dragStartPos = currentPos

    const size = devtoolsMode === 'left' ? contentEl.clientWidth : contentEl.clientHeight
    if (size <= 0) return

    const direction = devtoolsMode === 'left' ? -1 : 1
    devtoolsRatio = Math.min(0.8, Math.max(0.2, devtoolsRatio + (delta * direction) / size))
  }

  function handleDividerPointerUp(): void {
    dividerDragging = false
  }

  // DevTools should be hidden when tab is inactive or a modal overlay is open
  let devtoolsVisible = $derived(
    devtoolsOpen &&
      active &&
      !showPicker &&
      !savePrompt &&
      !favModalOpen &&
      !starDropdownOpen &&
      !favCtxMenu &&
      !appOverlayOpen,
  )

  function sendDevToolsBounds(): void {
    if (!devtoolsPlaceholder || !devtoolsVisible) return
    const rect = devtoolsPlaceholder.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0) return
    window.api.setBrowserDevToolsBounds(browserId, {
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    })
  }

  // Track devtools placeholder bounds and send to main process
  $effect(() => {
    if (!devtoolsOpen || !devtoolsPlaceholder) return

    const observer = new ResizeObserver(() => sendDevToolsBounds())
    observer.observe(devtoolsPlaceholder)
    sendDevToolsBounds()

    return () => observer.disconnect()
  })

  // Hide/show DevTools native view when tab becomes inactive/active or modal opens
  $effect(() => {
    if (devtoolsVisible) {
      sendDevToolsBounds()
    } else if (devtoolsOpen) {
      // Hide the native view but keep DevTools alive
      window.api.setBrowserDevToolsBounds(browserId, { x: 0, y: 0, width: 0, height: 0 })
    }
  })

  // --- Capture ---

  const ELEMENT_PICK_JS = `
    new Promise((resolve) => {
      const ov = document.createElement('div')
      ov.id = '__canopy_pick_overlay'
      ov.style.cssText = 'position:fixed;inset:0;z-index:999999;cursor:crosshair'
      const hl = document.createElement('div')
      hl.id = '__canopy_pick_highlight'
      hl.style.cssText = 'position:fixed;pointer-events:none;z-index:999998;'
        + 'border:2px solid #74c0fc;background:rgba(116,192,252,0.12);transition:all 0.05s'
      document.body.appendChild(hl)
      document.body.appendChild(ov)
      const cleanup = () => {
        ov.remove()
        hl.remove()
        document.removeEventListener('keydown', handler)
      }
      const handler = (e) => {
        if (e.key === 'Escape') {
          cleanup()
          resolve(null)
        }
      }
      ov.addEventListener('mousemove', (e) => {
        ov.style.pointerEvents = 'none'
        const el = document.elementFromPoint(e.clientX, e.clientY)
        ov.style.pointerEvents = 'auto'
        if (el && el !== document.body && el !== document.documentElement) {
          const r = el.getBoundingClientRect()
          hl.style.left = r.left+'px'; hl.style.top = r.top+'px'
          hl.style.width = r.width+'px'; hl.style.height = r.height+'px'
        }
      })
      ov.addEventListener('click', (e) => {
        e.preventDefault(); e.stopPropagation()
        ov.style.pointerEvents = 'none'
        const el = document.elementFromPoint(e.clientX, e.clientY)
        ov.style.pointerEvents = 'auto'
        cleanup()
        resolve(el ? el.outerHTML : null)
      })
      document.addEventListener('keydown', handler)
    })
  `

  const REGION_CAPTURE_JS = `
    new Promise((resolve) => {
      const ov = document.createElement('div')
      ov.id = '__canopy_capture_overlay'
      ov.style.cssText = 'position:fixed;inset:0;z-index:999999;cursor:crosshair'
      const sel = document.createElement('div')
      sel.style.cssText = 'position:fixed;pointer-events:none;z-index:999998;'
        + 'border:2px solid #74c0fc;background:rgba(116,192,252,0.15)'
      document.body.appendChild(sel)
      document.body.appendChild(ov)
      const cleanup = () => {
        ov.remove()
        sel.remove()
        document.removeEventListener('keydown', handler)
      }
      const handler = (e) => {
        if (e.key === 'Escape') {
          cleanup()
          resolve(null)
        }
      }
      let startX, startY, dragging = false
      ov.addEventListener('mousedown', (e) => {
        startX = e.clientX; startY = e.clientY; dragging = true
        sel.style.left = startX+'px'; sel.style.top = startY+'px'
        sel.style.width = '0px'; sel.style.height = '0px'
      })
      ov.addEventListener('mousemove', (e) => {
        if (!dragging) return
        const x = Math.min(e.clientX, startX), y = Math.min(e.clientY, startY)
        const w = Math.abs(e.clientX - startX), h = Math.abs(e.clientY - startY)
        sel.style.left = x+'px'; sel.style.top = y+'px'
        sel.style.width = w+'px'; sel.style.height = h+'px'
      })
      ov.addEventListener('mouseup', (e) => {
        if (!dragging) return
        dragging = false
        const x = Math.min(e.clientX, startX), y = Math.min(e.clientY, startY)
        const w = Math.abs(e.clientX - startX), h = Math.abs(e.clientY - startY)
        cleanup()
        if (w < 5 || h < 5) { resolve(null); return }
        resolve({ x, y, width: w, height: h })
      })
      document.addEventListener('keydown', handler)
    })
  `

  async function handleStartElementPick(): Promise<void> {
    const w = wv()
    if (!w) return
    pickMode = 'element'
    try {
      const html = await w.executeJavaScript(ELEMENT_PICK_JS)
      pickMode = 'none'
      if (html) sendToClaude('```html\n' + html + '\n```\n')
    } catch {
      pickMode = 'none'
    }
  }

  async function handleStartRegionCapture(): Promise<void> {
    const w = wv()
    if (!w) return
    pickMode = 'region'
    try {
      const bounds = await w.executeJavaScript(REGION_CAPTURE_JS)
      pickMode = 'none'
      if (!bounds || bounds.width < 5 || bounds.height < 5) return
      const image = await w.capturePage(bounds)
      const buffer = image.toPNG()
      const filePath = await window.api.saveBrowserCapture(buffer.buffer)
      if (filePath) sendToClaude(filePath + ' ')
    } catch {
      pickMode = 'none'
    }
  }

  function handleCancelPick(): void {
    const w = wv()
    if (w) {
      w.executeJavaScript(
        `
        document.getElementById('__canopy_pick_overlay')?.remove()
        document.getElementById('__canopy_pick_highlight')?.remove()
        document.getElementById('__canopy_capture_overlay')?.remove()
      `,
      ).catch(() => {})
    }
    pickMode = 'none'
  }

  // --- Send to Claude ---

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
      showPicker = true
    }
  }

  function deliverToSession(sessionId: string, payload: string): void {
    window.api.writePty(sessionId, payload)
    focusSessionByPtyId(sessionId)
    window.api.focusRendererWebContents().then(() => {
      window.dispatchEvent(new CustomEvent('canopy:focus-terminal', { detail: { sessionId } }))
    })
  }

  function handlePickerSelect(sessionId: string): void {
    showPicker = false
    if (pendingPayload) {
      deliverToSession(sessionId, pendingPayload)
      pendingPayload = null
    }
  }

  // --- Lifecycle ---

  onMount(() => {
    initBrowserSession(browserId)

    const unsubs = [
      window.api.onBrowserFaviconChanged((data) => {
        if (data.browserId === browserId) {
          handleBrowserFaviconChanged(browserId, data.favicon)
        }
      }),
      window.api.onBrowserFocused((data) => {
        if (data.browserId === browserId) {
          onFocus?.()
        }
      }),
      window.api.onBrowserDevToolsOpened((data) => {
        if (data.browserId === browserId && !devtoolsOpen) {
          devtoolsOpen = true
          updateDevToolsState(true)
        }
      }),
      window.api.onBrowserOpenUrl((data) => {
        if (data.browserId !== browserId) return
        const mode = prefs.urlOpenMode || 'ask'
        if (mode === 'canopy') {
          openTool('browser', worktreePath, { initialUrl: data.url })
        } else if (mode === 'system') {
          window.api.openExternal(data.url)
        } else {
          showUrlToast(data.url)
        }
      }),
    ]

    // Listen for app-level overlays (Preferences, Command Palette, etc.)
    const onAppOverlay = (e: Event): void => {
      appOverlayOpen = (e as CustomEvent<{ open: boolean }>).detail.open
    }
    window.addEventListener('canopy:app-overlay', onAppOverlay)

    return () => {
      alive = false
      unsubs.forEach((fn) => fn())
      window.removeEventListener('canopy:app-overlay', onAppOverlay)
      if (capturedCredsTimer) clearTimeout(capturedCredsTimer)
      if (devtoolsOpen) {
        window.api.closeBrowserDevTools(browserId).catch(() => {})
      }
      if (registeredElement) {
        window.api.teardownBrowserWebview(browserId).catch(() => {})
        registeredElement = null
      }
    }
  })

  // Setup browser webview
  $effect(() => {
    const w = wv()
    if (!w || registeredElement === webviewEl) return

    const onDomReady = async (): Promise<void> => {
      if (!alive || registeredElement === webviewEl) return
      registeredElement = webviewEl ?? null
      const wcId = w.getWebContentsId()
      await window.api.setupBrowserWebview(browserId, wcId)
      if (initialUrl) handleNavigate(initialUrl)
    }

    const onDidNavigate = (e: Event): void => {
      const url = (e as CustomEvent).url ?? w.getURL()
      if (url === 'about:blank') return
      checkCapturedCredentials()
      handleBrowserUrlChanged(browserId, url)
      updateBrowserPaneUrl(browserId, url)
      handleBrowserStateChanged(browserId, {
        canGoBack: w.canGoBack(),
        canGoForward: w.canGoForward(),
        isDevToolsOpen: devtoolsOpen,
        devToolsMode: devtoolsMode,
      })
    }

    const onTitleUpdated = (e: Event): void => {
      const title = (e as CustomEvent).title ?? w.getTitle()
      handleBrowserTitleChanged(browserId, title)
      onTitleChange(title)
    }

    const onStartLoading = (): void => handleBrowserLoadingChanged(browserId, true)
    const onStopLoading = (): void => {
      handleBrowserLoadingChanged(browserId, false)
      injectMouseNavigation()
      checkCredentials()
    }

    const onLoadFailed = (e: Event): void => {
      const ev = e as CustomEvent & {
        errorCode: number
        errorDescription: string
        validatedURL: string
        isMainFrame: boolean
      }
      if (!ev.isMainFrame || ev.errorCode === -3) return
      handleBrowserLoadFailed(browserId, ev.errorCode, ev.errorDescription, ev.validatedURL)
    }

    const onWebviewFocus = (): void => onFocus?.()

    const onConsoleMessage = (e: Event): void => {
      const msg = (e as CustomEvent & { message: string }).message
      if (typeof msg !== 'string') return
      match(msg)
        .with('__CANOPY_CREDS_READY__', () => {
          w.executeJavaScript(
            `
            (function() {
              const pw = document.querySelector('input[type="password"]')
              if (!pw || !pw.value) return null
              const form = pw.closest('form') || pw.parentElement
              const uf = form?.querySelector('input[type="email"],input[type="text"],input[name*="user"],input[name*="email"],input[name*="login"],input[autocomplete="username"]')
              return {
                username: uf?.value || '',
                password: pw.value,
                domain: location.host,
                title: document.title || '',
              }
            })()
          `,
          )
            .then((data) => {
              if (data) setLastCapturedCreds(data)
            })
            .catch(() => {})
        })
        .with('__CANOPY_PW_FIELD_FOUND__', () => {
          checkCredentials()
        })
        .with('__CANOPY_NAV__:back', () => {
          if (w.canGoBack()) w.goBack()
        })
        .with('__CANOPY_NAV__:forward', () => {
          if (w.canGoForward()) w.goForward()
        })
        .with(
          P.when((m) => m.startsWith('__CANOPY_FILL__:')),
          (m) => {
            handleAutofillCredential(m.slice('__CANOPY_FILL__:'.length))
          },
        )
        .otherwise(() => {})
    }

    w.addEventListener('dom-ready', onDomReady)
    w.addEventListener('did-navigate', onDidNavigate)
    w.addEventListener('did-navigate-in-page', onDidNavigate)
    w.addEventListener('page-title-updated', onTitleUpdated)
    w.addEventListener('did-start-loading', onStartLoading)
    w.addEventListener('did-stop-loading', onStopLoading)
    w.addEventListener('did-fail-load', onLoadFailed)
    w.addEventListener('focus', onWebviewFocus)
    w.addEventListener('console-message', onConsoleMessage)

    return () => {
      w.removeEventListener('dom-ready', onDomReady)
      w.removeEventListener('did-navigate', onDidNavigate)
      w.removeEventListener('did-navigate-in-page', onDidNavigate)
      w.removeEventListener('page-title-updated', onTitleUpdated)
      w.removeEventListener('did-start-loading', onStartLoading)
      w.removeEventListener('did-stop-loading', onStopLoading)
      w.removeEventListener('did-fail-load', onLoadFailed)
      w.removeEventListener('focus', onWebviewFocus)
      w.removeEventListener('console-message', onConsoleMessage)
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

  // Listen for Cmd+L focus event — only focused pane responds
  $effect(() => {
    if (!active || !focused) return
    const handler = (): void => toolbarComponent?.focusUrlBar()
    window.addEventListener('canopy:focus-url-bar', handler)
    return () => window.removeEventListener('canopy:focus-url-bar', handler)
  })

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
      isDevToolsOpen={devtoolsOpen}
      devToolsMode={devtoolsMode}
      {pickMode}
      onNavigate={handleNavigate}
      onBack={handleBack}
      onForward={handleForward}
      onReload={handleReload}
      onToggleDevTools={handleToggleDevTools}
      onSwitchDevToolsMode={handleSwitchDevToolsMode}
      hasAiSessions={aiSessionCount > 0}
      {activeDevice}
      {viewports}
      onStartElementPick={handleStartElementPick}
      onStartRegionCapture={handleStartRegionCapture}
      onCancelPick={handleCancelPick}
      onSetDevice={handleSetDevice}
      isFavorited={!!session?.url && session.url !== 'about:blank' && isFavorite(session.url)}
      onToggleFavorite={handleToggleFavorite}
    />
  {/if}

  {#if starDropdownOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="star-dropdown-backdrop" onclick={() => (starDropdownOpen = false)}></div>
    <div class="star-dropdown">
      <button class="star-dropdown-item star-dropdown-add" onclick={handleStarAddNew}>
        Add current page as new favorite
      </button>
      <button class="star-dropdown-item star-dropdown-remove" onclick={handleStarRemoveAll}>
        Remove all favorites for this site
      </button>
    </div>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="browser-content"
    class:devtools-left={devtoolsOpen && devtoolsMode === 'left'}
    bind:this={contentEl}
  >
    {#if session?.error}
      <BrowserError
        errorDescription={session.error.description}
        validatedURL={session.error.url}
        onRetry={handleReload}
        onDismiss={() => {
          if (session) session.error = null
        }}
      />
    {/if}
    {#if !session?.url}
      <div class="start-page">
        <h2 class="start-title">Favorites</h2>
        {#if getFavorites().length > 0}
          <div class="favorites-grid">
            {#each getFavorites() as fav, i (fav.url)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <button
                class="favorite-card"
                class:drag-over={favDropIndex === i && favDragIndex !== i}
                draggable="true"
                onclick={() => handleNavigate(fav.url)}
                oncontextmenu={(e) => handleFavContextMenu(e, i)}
                ondragstart={(e) => handleFavDragStart(e, i)}
                ondragover={(e) => handleFavDragOver(e, i)}
                ondrop={(e) => handleFavDrop(e, i)}
                ondragend={() => {
                  favDragIndex = null
                  favDropIndex = null
                }}
              >
                <div class="favorite-icon">
                  {#if fav.favicon}
                    <img src={fav.favicon} alt="" class="favorite-favicon" draggable="false" />
                  {:else}
                    <span class="favorite-initials">
                      {fav.name.slice(0, 2).toUpperCase()}
                    </span>
                  {/if}
                </div>
                <span class="favorite-name">{fav.name}</span>
              </button>
            {/each}
          </div>
        {:else}
          <p class="start-hint">
            Navigate to a page and click the star to add it to your favorites
          </p>
        {/if}
      </div>
    {/if}

    {#if favCtxMenu}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="fav-ctx-backdrop" onclick={() => (favCtxMenu = null)}></div>
      <div class="fav-ctx-menu" style="left:{favCtxMenu.x}px;top:{favCtxMenu.y}px">
        <button class="fav-ctx-item" onclick={() => handleFavEdit(favCtxMenu.index)}>Edit</button>
        <button class="fav-ctx-item" onclick={() => handleFavDuplicate(favCtxMenu.index)}
          >Duplicate</button
        >
        <button
          class="fav-ctx-item fav-ctx-delete"
          onclick={() => handleFavDelete(favCtxMenu.index)}>Delete</button
        >
      </div>
    {/if}

    {#if favModalOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="fav-modal-backdrop" onclick={() => (favModalOpen = false)}>
        <div class="fav-modal" onclick={(e) => e.stopPropagation()}>
          <h3 class="fav-modal-title">
            {favModalMode === 'edit' ? 'Edit Favorite' : 'Add Favorite'}
          </h3>
          <label class="fav-modal-label">
            Name
            <input class="fav-modal-input" bind:value={favName} />
          </label>
          <label class="fav-modal-label">
            URL
            <input class="fav-modal-input" bind:value={favUrl} />
          </label>
          <div class="fav-modal-actions">
            <button class="fav-modal-btn fav-modal-cancel" onclick={() => (favModalOpen = false)}
              >Cancel</button
            >
            <button class="fav-modal-btn fav-modal-save" onclick={handleFavModalSave}>
              {favModalMode === 'edit' ? 'Save' : 'Add'}
            </button>
          </div>
        </div>
      </div>
    {/if}
    <div
      class="webview-wrapper"
      class:device-mode={activeDevice !== null}
      class:webview-hidden={!session?.url}
      bind:this={wrapperEl}
    >
      <div
        class:device-frame={activeDevice !== null}
        class:desktop-frame={activeDevice === null}
        style={activeDevice && viewports[activeDevice]
          ? `width:${Math.round(viewports[activeDevice].width * deviceScale)}px;height:${Math.round(viewports[activeDevice].height * deviceScale)}px`
          : ''}
      >
        <webview
          bind:this={webviewEl}
          src="about:blank"
          partition="persist:browser"
          webpreferences="contextIsolation=yes,nodeIntegration=no,sandbox=yes"
          allowpopups
          class="browser-webview"
          class:hidden={!!session?.error && !activeDevice}
          class:no-events={dragState.isDragging}
          style={activeDevice && viewports[activeDevice]
            ? `width:${viewports[activeDevice].width}px;height:${viewports[activeDevice].height}px;transform:scale(${deviceScale});transform-origin:top left`
            : ''}
        ></webview>
      </div>
      {#if activeDevice}
        <span class="device-label">{activeDevice}</span>
      {/if}
    </div>

    {#if devtoolsOpen}
      <div
        class="devtools-divider"
        class:dragging={dividerDragging}
        onpointerdown={handleDividerPointerDown}
        onpointermove={handleDividerPointerMove}
        onpointerup={handleDividerPointerUp}
        onpointercancel={handleDividerPointerUp}
      ></div>
      <div
        class="devtools-placeholder"
        bind:this={devtoolsPlaceholder}
        style="flex: 0 0 {(1 - devtoolsRatio) * 100}%"
      ></div>
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
      }}
    />
  {/if}
{/if}

{#if savePrompt}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="save-modal-backdrop" onclick={handleDismissSavePrompt}>
    <div class="save-modal" onclick={(e) => e.stopPropagation()}>
      <h3 class="save-modal-title">Save Password</h3>
      <p class="save-modal-domain">{savePrompt.domain}</p>
      <label class="save-modal-label">
        Username / Email
        <input class="save-modal-input" bind:value={savePrompt.username} />
      </label>
      <label class="save-modal-label">
        Password
        <input class="save-modal-input" type="password" bind:value={savePrompt.password} />
      </label>
      <div class="save-modal-actions">
        <button class="save-modal-btn save-modal-dismiss" onclick={handleDismissSavePrompt}>
          Don't save
        </button>
        <button class="save-modal-btn save-modal-save" onclick={handleSaveCredential}>
          Save Password
        </button>
      </div>
    </div>
  </div>
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
    display: flex;
    flex-direction: column;
  }

  .browser-content.devtools-left {
    flex-direction: row-reverse;
  }

  .favorite-card.drag-over {
    border-color: rgba(116, 192, 252, 0.5);
    background: rgba(116, 192, 252, 0.08);
  }

  .fav-ctx-backdrop {
    position: fixed;
    inset: 0;
    z-index: 199;
  }

  .fav-ctx-menu {
    position: fixed;
    z-index: 200;
    min-width: 120px;
    padding: 4px;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }

  .fav-ctx-item {
    display: block;
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

  .fav-ctx-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .fav-ctx-delete {
    color: rgba(255, 120, 120, 0.8);
  }

  .fav-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 300;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .fav-modal {
    width: 320px;
    padding: 20px;
    background: rgb(35, 35, 35);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .fav-modal-title {
    font-size: 15px;
    font-weight: 600;
    color: #e0e0e0;
    margin: 0;
  }

  .fav-modal-label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .fav-modal-input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .fav-modal-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .fav-modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .fav-modal-btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
  }

  .fav-modal-cancel {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .fav-modal-save {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .fav-modal-save:hover {
    background: rgba(116, 192, 252, 0.3);
  }

  .webview-hidden {
    display: none !important;
  }

  .star-dropdown-backdrop {
    position: fixed;
    inset: 0;
    z-index: 99;
  }

  .star-dropdown {
    position: absolute;
    right: 120px;
    top: 38px;
    min-width: 220px;
    padding: 4px;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
    z-index: 100;
  }

  .star-dropdown-item {
    display: block;
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.8);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }

  .star-dropdown-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .star-dropdown-remove {
    color: rgba(255, 130, 130, 0.8);
  }

  .save-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 300;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .save-modal {
    width: 340px;
    padding: 20px;
    background: rgb(35, 35, 35);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .save-modal-title {
    font-size: 15px;
    font-weight: 600;
    color: #e0e0e0;
    margin: 0;
  }

  .save-modal-domain {
    font-size: 12px;
    color: rgba(116, 192, 252, 0.7);
    margin: 0;
    font-family: monospace;
  }

  .save-modal-label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .save-modal-input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .save-modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .save-modal-btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
  }

  .save-modal-dismiss {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .save-modal-save {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .save-modal-save:hover {
    background: rgba(116, 192, 252, 0.3);
  }

  .start-page {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 20px;
    padding: 32px;
  }

  .start-title {
    font-size: 14px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.4);
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .start-hint {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.25);
    margin: 0;
  }

  .favorites-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    justify-content: center;
    max-width: 500px;
  }

  .favorite-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: 80px;
    padding: 12px 8px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
    cursor: pointer;
    color: rgba(255, 255, 255, 0.7);
    font-family: inherit;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .favorite-card:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .favorite-icon {
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    overflow: hidden;
  }

  .favorite-favicon {
    width: 20px;
    height: 20px;
  }

  .favorite-initials {
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.5);
  }

  .favorite-name {
    font-size: 11px;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }

  .webview-wrapper {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: flex;
  }

  .webview-wrapper.device-mode {
    justify-content: center;
    align-items: center;
    flex-direction: column;
    background: rgba(0, 0, 0, 0.4);
    gap: 6px;
    overflow: hidden;
  }

  .browser-webview {
    flex: 1;
    min-height: 0;
    min-width: 0;
    border: none;
  }

  .desktop-frame {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: flex;
  }

  .device-frame {
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    overflow: hidden;
    flex-shrink: 0;
  }

  .device-frame .browser-webview {
    width: 100%;
    height: 100%;
  }

  .browser-webview.hidden {
    display: none;
  }

  .browser-webview.no-events {
    pointer-events: none;
  }

  .device-label {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.35);
  }

  .devtools-divider {
    flex-shrink: 0;
    background: rgba(255, 255, 255, 0.08);
    transition: background 0.1s;
  }

  .devtools-divider:hover,
  .devtools-divider.dragging {
    background: rgba(116, 192, 252, 0.3);
  }

  .browser-content:not(.devtools-left) .devtools-divider {
    height: 4px;
    cursor: row-resize;
  }

  .browser-content.devtools-left .devtools-divider {
    width: 4px;
    cursor: col-resize;
  }

  .devtools-placeholder {
    min-height: 0;
    min-width: 0;
  }
</style>
