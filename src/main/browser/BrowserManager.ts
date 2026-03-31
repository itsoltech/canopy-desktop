import { webContents, WebContentsView, Menu, session, type BrowserWindow } from 'electron'
import type { WebContents } from 'electron'
import { join } from 'path'
import { writeFileSync } from 'fs'
import { randomUUID } from 'crypto'
import os from 'os'

/**
 * findWebContents() can return undefined for <webview> guest contents
 * in some Electron versions. This helper scans getAllWebContents() as fallback.
 */
function findWebContents(id: number): WebContents | undefined {
  return findWebContents(id) ?? webContents.getAllWebContents().find((wc) => wc.id === id)
}

interface WebviewEntry {
  webContentsId: number
  win: BrowserWindow
  sender: WebContents
  devToolsMode: 'bottom' | 'left'
  devToolsView?: WebContentsView
}

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

const BROWSER_PARTITION = 'persist:browser'

export class BrowserManager {
  private entries = new Map<string, WebviewEntry>()
  private guestContents = new Map<number, WebContents>()
  private partitionReady = false

  /**
   * Track webview guest webContents as they attach to a window.
   * Must be called for each BrowserWindow so we can reliably
   * get guest webContents (webContents.fromId doesn't work for guests).
   */
  trackWindow(win: BrowserWindow): void {
    win.webContents.on('did-attach-webview', (_event, guestWc) => {
      this.guestContents.set(guestWc.id, guestWc)
      guestWc.on('destroyed', () => {
        this.guestContents.delete(guestWc.id)
      })
    })

    // Mouse back/forward buttons (button 4/5)
    win.on('app-command', (_event, command) => {
      // Find the focused browser webview in this window
      for (const entry of this.entries.values()) {
        if (entry.win !== win) continue
        const wc = this.guestContents.get(entry.webContentsId)
        if (!wc || wc.isDestroyed() || !wc.isFocused()) continue
        if (command === 'browser-backward' && wc.canGoBack()) {
          wc.goBack()
        } else if (command === 'browser-forward' && wc.canGoForward()) {
          wc.goForward()
        }
        break
      }
    })
  }

  /** One-time setup: permission handler on the shared browser session. */
  ensurePartition(): void {
    if (this.partitionReady) return
    this.partitionReady = true

    const browserSession = session.fromPartition(BROWSER_PARTITION)

    // Block ALL permission requests (camera, mic, geolocation, etc.)
    browserSession.setPermissionRequestHandler((_wc, _permission, callback) => {
      callback(false)
    })
  }

  /**
   * Register a renderer-created <webview> for keyboard interception,
   * popup blocking, navigation filtering, and favicon forwarding.
   */
  setup(browserId: string, wcId: number, win: BrowserWindow, sender: WebContents): void {
    const wc = this.guestContents.get(wcId) ?? findWebContents(wcId)
    if (!wc) return

    const entry: WebviewEntry = {
      webContentsId: wcId,
      win,
      sender,
      devToolsMode: 'bottom',
    }
    this.entries.set(browserId, entry)

    // Block popups
    wc.setWindowOpenHandler(() => ({ action: 'deny' }))

    // Only allow http(s) navigation
    wc.on('will-navigate', (event, url) => {
      try {
        const parsed = new URL(url)
        if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
          event.preventDefault()
        }
      } catch {
        event.preventDefault()
      }
    })

    // Intercept keyboard shortcuts so they reach the main renderer
    wc.on('before-input-event', (event, input) => {
      if (!input.meta && !input.control) return

      // Cmd+R: reload the page locally
      if (input.key === 'r' && input.type === 'keyDown') {
        event.preventDefault()
        wc.reload()
        return
      }

      if (
        APP_SHORTCUTS.has(input.key) ||
        (input.shift && (input.key === 'I' || input.key === 'N'))
      ) {
        event.preventDefault()
        if (!win.isDestroyed()) {
          win.webContents.focus()
          win.webContents.sendInputEvent({
            type: input.type === 'keyUp' ? 'keyUp' : 'keyDown',
            keyCode: input.key,
            modifiers: [
              ...(input.meta ? (['meta'] as const) : []),
              ...(input.control ? (['ctrl'] as const) : []),
              ...(input.shift ? (['shift'] as const) : []),
              ...(input.alt ? (['alt'] as const) : []),
            ],
          })
        }
      }
    })

    // Forward focus so renderer can track active pane
    wc.on('focus', () => {
      this.sendToRenderer(browserId, 'browser:focused', { browserId })
    })

    // Favicon: fetch via session.fetch to bypass CORS, send as data URL
    wc.on('page-favicon-updated', (_event, favicons) => {
      const faviconUrl = favicons[0]
      if (!faviconUrl) {
        this.sendToRenderer(browserId, 'browser:faviconChanged', {
          browserId,
          favicon: null,
        })
        return
      }
      wc.session
        .fetch(faviconUrl)
        .then(async (response) => {
          if (!response.ok) return
          const buffer = await response.arrayBuffer()
          const contentType = response.headers.get('content-type') ?? 'image/x-icon'
          const base64 = Buffer.from(buffer).toString('base64')
          this.sendToRenderer(browserId, 'browser:faviconChanged', {
            browserId,
            favicon: `data:${contentType};base64,${base64}`,
          })
        })
        .catch(() => {
          // Favicon fetch failed, ignore
        })
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
  }

  teardown(browserId: string): void {
    const entry = this.entries.get(browserId)
    if (entry) {
      // Close DevTools
      const browserWc =
        this.guestContents.get(entry.webContentsId) ?? findWebContents(entry.webContentsId)
      if (browserWc && !browserWc.isDestroyed() && browserWc.isDevToolsOpened()) {
        browserWc.closeDevTools()
      }
      // Destroy the view on full teardown
      if (entry.devToolsView) {
        try {
          entry.win.contentView.removeChildView(entry.devToolsView)
        } catch {
          // Already removed
        }
      }
    }
    this.entries.delete(browserId)
  }

  /**
   * Open DevTools embedded via a WebContentsView.
   * Creates a fresh WebContentsView (guaranteed non-navigated), uses
   * setDevToolsWebContents to direct DevTools output into it, then
   * adds it to the window. Renderer controls bounds via setDevToolsBounds.
   */
  openDevTools(browserId: string): void {
    const entry = this.entries.get(browserId)
    if (!entry) return

    const browserWc =
      this.guestContents.get(entry.webContentsId) ?? findWebContents(entry.webContentsId)
    if (!browserWc || browserWc.isDestroyed()) return

    if (!entry.devToolsView) {
      // First open: create view, set as DevTools target (one-time)
      const devView = new WebContentsView()
      entry.devToolsView = devView
      browserWc.setDevToolsWebContents(devView.webContents)
      entry.win.contentView.addChildView(devView)
      devView.setBounds({ x: 0, y: 0, width: 0, height: 0 })
      browserWc.openDevTools({ mode: 'detach' })
    }
    // Bounds will be set by renderer via setDevToolsBounds
  }

  closeDevTools(browserId: string): void {
    const entry = this.entries.get(browserId)
    if (!entry?.devToolsView) return
    // Don't actually close DevTools — just hide the view.
    // setDevToolsWebContents can't be re-called, so we keep DevTools
    // alive internally and toggle visibility via bounds.
    entry.devToolsView.setBounds({ x: 0, y: 0, width: 0, height: 0 })
  }

  setDeviceEmulation(
    browserId: string,
    device: { width: number; height: number; scaleFactor: number; mobile: boolean } | null,
  ): void {
    const entry = this.entries.get(browserId)
    if (!entry) return

    const wc = this.guestContents.get(entry.webContentsId) ?? findWebContents(entry.webContentsId)
    if (!wc || wc.isDestroyed()) return

    if (!wc.debugger.isAttached()) {
      try {
        wc.debugger.attach('1.3')
      } catch {
        return
      }
    }

    if (device) {
      wc.debugger
        .sendCommand('Emulation.setDeviceMetricsOverride', {
          width: device.width,
          height: device.height,
          deviceScaleFactor: device.scaleFactor,
          mobile: device.mobile,
        })
        .catch(() => {})
    } else {
      wc.debugger.sendCommand('Emulation.clearDeviceMetricsOverride').catch(() => {})
    }
  }

  setDevToolsBounds(
    browserId: string,
    bounds: { x: number; y: number; width: number; height: number },
  ): void {
    const entry = this.entries.get(browserId)
    if (!entry?.devToolsView) return
    entry.devToolsView.setBounds(bounds)
  }

  saveCaptureFile(pngBuffer: Buffer): string {
    const filePath = join(os.tmpdir(), `canopy-capture-${randomUUID()}.png`)
    writeFileSync(filePath, pngBuffer)
    return filePath
  }

  teardownAllForWindow(win: BrowserWindow): void {
    for (const [id, entry] of this.entries) {
      if (entry.win === win) {
        this.entries.delete(id)
      }
    }
  }

  private sendToRenderer(browserId: string, channel: string, data: unknown): void {
    const entry = this.entries.get(browserId)
    if (entry && !entry.sender.isDestroyed()) {
      entry.sender.send(channel, data)
    }
  }
}
