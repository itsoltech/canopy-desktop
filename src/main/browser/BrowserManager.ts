import { WebContentsView, Menu, type BrowserWindow } from 'electron'
import type { WebContents } from 'electron'
import { join } from 'path'
import { writeFileSync } from 'fs'
import { randomUUID } from 'crypto'
import os from 'os'

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

    // Only allow http(s) navigation
    view.webContents.on('will-navigate', (event, url) => {
      try {
        const parsed = new URL(url)
        if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
          event.preventDefault()
        }
      } catch {
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

    // Forward focus event so renderer can track active pane
    const wc = view.webContents

    wc.on('focus', () => {
      this.sendToRenderer(id, 'browser:focused', { browserId: id })
    })

    // Forward navigation events to renderer

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

    // Auto-prefix with scheme if missing
    let finalUrl = url
    if (!/^https?:\/\//i.test(finalUrl)) {
      // Use http:// for localhost/127.0.0.1 (dev servers), https:// for everything else
      const isLocal = /^(localhost|127\.0\.0\.1)(:\d+)?(\/|$)/i.test(finalUrl)
      finalUrl = (isLocal ? 'http://' : 'https://') + finalUrl
    }

    // Only allow http(s)
    try {
      const parsed = new URL(finalUrl)
      if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return
    } catch {
      return
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
    // When hiding, return keyboard focus to the main renderer so the
    // terminal (xterm.js) can receive keystrokes immediately.
    if (!visible && !entry.win.isDestroyed()) {
      entry.win.webContents.focus()
    }
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

  async startElementPick(id: string): Promise<string | null> {
    const entry = this.views.get(id)
    if (!entry) return null

    const js = `
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
          ov.remove(); hl.remove()
          resolve(el ? el.outerHTML : null)
        })
        document.addEventListener('keydown', function handler(e) {
          if (e.key === 'Escape') {
            ov.remove(); hl.remove()
            document.removeEventListener('keydown', handler)
            resolve(null)
          }
        })
      })
    `
    return entry.view.webContents.executeJavaScript(js)
  }

  async startRegionCapture(id: string): Promise<string | null> {
    const entry = this.views.get(id)
    if (!entry) return null

    const js = `
      new Promise((resolve) => {
        const ov = document.createElement('div')
        ov.id = '__canopy_capture_overlay'
        ov.style.cssText = 'position:fixed;inset:0;z-index:999999;cursor:crosshair'
        const sel = document.createElement('div')
        sel.style.cssText = 'position:fixed;pointer-events:none;z-index:999998;'
          + 'border:2px solid #74c0fc;background:rgba(116,192,252,0.15)'
        document.body.appendChild(sel)
        document.body.appendChild(ov)
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
          ov.remove(); sel.remove()
          if (w < 5 || h < 5) { resolve(null); return }
          resolve({ x, y, width: w, height: h })
        })
        document.addEventListener('keydown', function handler(e) {
          if (e.key === 'Escape') {
            ov.remove(); sel.remove()
            document.removeEventListener('keydown', handler)
            resolve(null)
          }
        })
      })
    `
    const bounds = await entry.view.webContents.executeJavaScript(js)
    if (!bounds) return null

    const image = await entry.view.webContents.capturePage(bounds)
    const filePath = join(os.tmpdir(), `canopy-capture-${randomUUID()}.png`)
    writeFileSync(filePath, image.toPNG())
    return filePath
  }

  async capturePageFull(id: string): Promise<string | null> {
    const entry = this.views.get(id)
    if (!entry) return null
    const image = await entry.view.webContents.capturePage()
    return image.toDataURL()
  }

  cancelPick(id: string): void {
    const entry = this.views.get(id)
    if (!entry) return
    entry.view.webContents
      .executeJavaScript(
        `
      document.getElementById('__canopy_pick_overlay')?.remove()
      document.getElementById('__canopy_pick_highlight')?.remove()
      document.getElementById('__canopy_capture_overlay')?.remove()
    `,
      )
      .catch(() => {})
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
