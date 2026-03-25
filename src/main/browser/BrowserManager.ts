import { WebContentsView, Menu, type BrowserWindow } from 'electron'
import type { WebContents } from 'electron'

interface BrowserViewEntry {
  view: WebContentsView
  win: BrowserWindow
  sender: WebContents
  visible: boolean
  devToolsMode: 'bottom' | 'right'
}

export class BrowserManager {
  private views = new Map<string, BrowserViewEntry>()

  create(id: string, win: BrowserWindow, sender: WebContents): void {
    const view = new WebContentsView({
      webPreferences: {
        sandbox: true,
        contextIsolation: true,
        nodeIntegration: false,
      },
    })

    win.contentView.addChildView(view)
    view.setBounds({ x: 0, y: 0, width: 0, height: 0 })

    const entry: BrowserViewEntry = {
      view,
      win,
      sender,
      visible: false,
      devToolsMode: 'bottom',
    }
    this.views.set(id, entry)

    // Block popups
    view.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))

    // Block dangerous URL schemes
    view.webContents.on('will-navigate', (event, url) => {
      if (url.startsWith('file://') || url.startsWith('javascript:')) {
        event.preventDefault()
      }
    })

    // Intercept keyboard shortcuts so they go to the main window, not the webview
    const APP_SHORTCUTS = new Set([
      'w',
      't',
      'T',
      'k',
      'b',
      'd',
      'D',
      ',',
      'l',
      'o',
      '1',
      '2',
      '3',
      '4',
      '5',
      '6',
      '7',
      '8',
      '9',
    ])

    view.webContents.on('before-input-event', (event, input) => {
      if (!input.meta && !input.control) return

      // Cmd+R: reload the page (handle directly, don't forward)
      if (input.key === 'r' && input.type === 'keyDown') {
        event.preventDefault()
        view.webContents.reload()
        return
      }

      if (
        APP_SHORTCUTS.has(input.key) ||
        (input.shift && (input.key === 'I' || input.key === 'N'))
      ) {
        event.preventDefault()
        // Re-dispatch to the main window's renderer so Svelte handles it
        if (!win.isDestroyed()) {
          win.webContents.focus()
          // Send a synthetic key event to the renderer
          win.webContents.sendInputEvent({
            type: input.type === 'keyUp' ? 'keyUp' : 'keyDown',
            keyCode: input.key,
            modifiers: [
              ...(input.meta ? ['meta' as const] : []),
              ...(input.control ? ['ctrl' as const] : []),
              ...(input.shift ? ['shift' as const] : []),
              ...(input.alt ? ['alt' as const] : []),
            ],
          })
        }
      }
    })

    // Forward navigation events to renderer
    const wc = view.webContents

    wc.on('did-navigate', (_event, url) => {
      this.sendToRenderer(id, 'browser:urlChanged', { browserId: id, url })
      this.sendStateChanged(id)
    })

    wc.on('did-navigate-in-page', (_event, url) => {
      this.sendToRenderer(id, 'browser:urlChanged', { browserId: id, url })
      this.sendStateChanged(id)
    })

    wc.on('page-title-updated', (_event, title) => {
      this.sendToRenderer(id, 'browser:titleChanged', { browserId: id, title })
    })

    wc.on('page-favicon-updated', (_event, favicons) => {
      const faviconUrl = favicons[0]
      if (!faviconUrl) {
        this.sendToRenderer(id, 'browser:faviconChanged', { browserId: id, favicon: null })
        return
      }
      // Fetch favicon and convert to data URL so renderer CSP doesn't block it
      wc.session
        .fetch(faviconUrl)
        .then(async (response) => {
          if (!response.ok) return
          const buffer = await response.arrayBuffer()
          const contentType = response.headers.get('content-type') ?? 'image/x-icon'
          const base64 = Buffer.from(buffer).toString('base64')
          this.sendToRenderer(id, 'browser:faviconChanged', {
            browserId: id,
            favicon: `data:${contentType};base64,${base64}`,
          })
        })
        .catch(() => {
          // Favicon fetch failed, ignore
        })
    })

    wc.on('did-start-loading', () => {
      this.sendToRenderer(id, 'browser:loadingChanged', { browserId: id, isLoading: true })
    })

    wc.on('did-stop-loading', () => {
      this.sendToRenderer(id, 'browser:loadingChanged', { browserId: id, isLoading: false })
    })

    wc.on('did-fail-load', (_event, errorCode, errorDescription, validatedURL, isMainFrame) => {
      if (!isMainFrame) return
      // Ignore aborted loads (user navigated away)
      if (errorCode === -3) return
      this.sendToRenderer(id, 'browser:loadFailed', {
        browserId: id,
        errorCode,
        errorDescription,
        validatedURL,
      })
    })

    wc.on('devtools-opened', () => {
      this.sendStateChanged(id)
    })

    wc.on('devtools-closed', () => {
      this.sendStateChanged(id)
    })

    // Context menu with Inspect Element
    wc.on('context-menu', (_event, params) => {
      const menu = Menu.buildFromTemplate([
        {
          label: 'Back',
          enabled: wc.canGoBack(),
          click: () => wc.goBack(),
        },
        {
          label: 'Forward',
          enabled: wc.canGoForward(),
          click: () => wc.goForward(),
        },
        {
          label: 'Reload',
          click: () => wc.reload(),
        },
        { type: 'separator' },
        {
          label: 'Copy',
          role: 'copy',
          enabled: params.editFlags.canCopy,
        },
        {
          label: 'Paste',
          role: 'paste',
          enabled: params.editFlags.canPaste,
        },
        { type: 'separator' },
        {
          label: 'Inspect Element',
          click: () => {
            wc.inspectElement(params.x, params.y)
          },
        },
      ])
      menu.popup()
    })

    // Deny permission requests (camera, mic, geolocation, etc.)
    wc.session.setPermissionRequestHandler((_wc, _permission, callback) => {
      callback(false)
    })
  }

  destroy(id: string): void {
    const entry = this.views.get(id)
    if (!entry) return

    try {
      if (entry.view.webContents && !entry.view.webContents.isDestroyed()) {
        entry.view.webContents.closeDevTools()
      }
      if (!entry.win.isDestroyed()) {
        entry.win.contentView.removeChildView(entry.view)
      }
    } catch {
      // View or window already destroyed
    }

    this.views.delete(id)
  }

  navigate(id: string, url: string): void {
    const entry = this.views.get(id)
    if (!entry) return

    // Block dangerous schemes
    if (url.startsWith('file://') || url.startsWith('javascript:')) return

    // Auto-prefix with scheme if missing
    let finalUrl = url
    if (!/^https?:\/\//i.test(finalUrl)) {
      // Use http:// for localhost/127.0.0.1 (dev servers), https:// for everything else
      const isLocal = /^(localhost|127\.0\.0\.1)(:\d+)?(\/|$)/i.test(finalUrl)
      finalUrl = (isLocal ? 'http://' : 'https://') + finalUrl
    }

    entry.view.webContents.loadURL(finalUrl)
  }

  goBack(id: string): void {
    const entry = this.views.get(id)
    if (entry?.view.webContents.canGoBack()) {
      entry.view.webContents.goBack()
    }
  }

  goForward(id: string): void {
    const entry = this.views.get(id)
    if (entry?.view.webContents.canGoForward()) {
      entry.view.webContents.goForward()
    }
  }

  reload(id: string): void {
    const entry = this.views.get(id)
    if (entry) {
      entry.view.webContents.reload()
    }
  }

  setBounds(id: string, bounds: { x: number; y: number; width: number; height: number }): void {
    const entry = this.views.get(id)
    if (!entry) return
    entry.view.setBounds(bounds)
  }

  setVisible(id: string, visible: boolean): void {
    const entry = this.views.get(id)
    if (!entry) return
    entry.visible = visible
    entry.view.setVisible(visible)
  }

  toggleDevTools(id: string, mode?: 'bottom' | 'right'): void {
    const entry = this.views.get(id)
    if (!entry) return

    if (mode) {
      entry.devToolsMode = mode
    }

    const wc = entry.view.webContents
    if (wc.isDevToolsOpened()) {
      wc.closeDevTools()
    } else {
      wc.openDevTools({ mode: entry.devToolsMode })
    }
  }

  getState(id: string): {
    url: string
    title: string
    canGoBack: boolean
    canGoForward: boolean
    isLoading: boolean
    isDevToolsOpen: boolean
    devToolsMode: 'bottom' | 'right'
  } | null {
    const entry = this.views.get(id)
    if (!entry) return null

    const wc = entry.view.webContents
    return {
      url: wc.getURL(),
      title: wc.getTitle(),
      canGoBack: wc.canGoBack(),
      canGoForward: wc.canGoForward(),
      isLoading: wc.isLoading(),
      isDevToolsOpen: wc.isDevToolsOpened(),
      devToolsMode: entry.devToolsMode,
    }
  }

  destroyAllForWindow(win: BrowserWindow): void {
    for (const [id, entry] of this.views) {
      if (entry.win === win) {
        this.destroy(id)
      }
    }
  }

  private sendToRenderer(id: string, channel: string, data: unknown): void {
    const entry = this.views.get(id)
    if (entry && !entry.sender.isDestroyed()) {
      entry.sender.send(channel, data)
    }
  }

  private sendStateChanged(id: string): void {
    const entry = this.views.get(id)
    if (!entry) return

    const wc = entry.view.webContents
    this.sendToRenderer(id, 'browser:stateChanged', {
      browserId: id,
      canGoBack: wc.canGoBack(),
      canGoForward: wc.canGoForward(),
      isDevToolsOpen: wc.isDevToolsOpened(),
      devToolsMode: entry.devToolsMode,
    })
  }
}
