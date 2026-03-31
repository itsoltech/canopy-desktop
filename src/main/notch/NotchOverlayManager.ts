import { app, BrowserWindow, ipcMain, nativeImage, screen } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'
import type { AgentSessionManager } from '../agents/AgentSessionManager'
import type { WindowManager } from '../WindowManager'
import type { NotchSessionStatus, NotchOverlayState, SessionStatusType } from './types'

// Window is oversized so CSS animations can expand freely inside it.
// Transparent pixels are click-through on macOS.
const WINDOW_WIDTH = 560
const WINDOW_HEIGHT = 650

export class NotchOverlayManager {
  private overlayWindow: BrowserWindow | null = null
  private sessions = new Map<string, NotchSessionStatus>()
  private previousStatuses = new Map<string, SessionStatusType>()
  private pendingPeekIds = new Set<string>()
  private displayChangeHandler: (() => void) | null = null
  private statusChangeHandler: ((status: NotchSessionStatus) => void) | null = null
  private sessionDestroyedHandler: ((id: string) => void) | null = null

  constructor(
    private agentSessionManager: AgentSessionManager,
    private windowManager: WindowManager,
  ) {}

  initialize(): void {
    if (this.overlayWindow) return // already initialized
    if (!this.hasNotch()) return

    const focusedWin = BrowserWindow.getFocusedWindow()
    this.createOverlayWindow()
    // Panel windows cause macOS to hide the dock icon; restore it then refocus.
    // After restoring, force-set the icon so the Dock refreshes its cached entry.
    if (process.platform === 'darwin') {
      app.dock?.show().then(() => {
        const iconPath = app.isPackaged
          ? join(process.resourcesPath, 'electron.icns')
          : join(app.getAppPath(), 'build', 'icon.icns')
        const icon = nativeImage.createFromPath(iconPath)
        if (!icon.isEmpty()) {
          app.dock?.setIcon(icon)
        }
        if (focusedWin && !focusedWin.isDestroyed()) focusedWin.focus()
      })
    }
    this.bindAgentEvents()
    this.bindIpcHandlers()
    this.watchDisplayChanges()
  }

  dispose(): void {
    if (this.statusChangeHandler) {
      this.agentSessionManager.removeListener('statusChange', this.statusChangeHandler)
      this.statusChangeHandler = null
    }
    if (this.sessionDestroyedHandler) {
      this.agentSessionManager.removeListener('sessionDestroyed', this.sessionDestroyedHandler)
      this.sessionDestroyedHandler = null
    }
    if (this.displayChangeHandler) {
      screen.removeListener('display-metrics-changed', this.displayChangeHandler)
      this.displayChangeHandler = null
    }

    ipcMain.removeHandler('notch:focusSession')
    ipcMain.removeAllListeners('notch:setMouseIgnore')

    if (this.overlayWindow && !this.overlayWindow.isDestroyed()) {
      this.overlayWindow.destroy()
    }
    this.overlayWindow = null
    this.sessions.clear()
    this.previousStatuses.clear()
    this.pendingPeekIds.clear()
  }

  private hasNotch(): boolean {
    if (process.platform === 'darwin') {
      const primary = screen.getPrimaryDisplay()
      const menuBarHeight = primary.workArea.y - primary.bounds.y
      return menuBarHeight > 28
    }
    // Windows: simulated notch (no physical notch to detect)
    return process.platform === 'win32'
  }

  /** Estimate notch dimensions from display geometry.
   *  macOS: Height = menu bar height (exact). Width = height * 5.5 (aspect ratio heuristic).
   *  Windows: Fixed dimensions for simulated notch. */
  private getNotchDimensions(): { width: number; height: number } {
    if (process.platform === 'darwin') {
      const primary = screen.getPrimaryDisplay()
      const height = primary.workArea.y - primary.bounds.y
      const width = Math.round(height * 5.5)
      return { width, height }
    }
    return { width: 200, height: 32 }
  }

  private createOverlayWindow(): void {
    const isMac = process.platform === 'darwin'
    const primary = screen.getPrimaryDisplay()
    const x = Math.round(primary.workArea.x + primary.workArea.width / 2 - WINDOW_WIDTH / 2)
    const y = 0

    this.overlayWindow = new BrowserWindow({
      x,
      y,
      width: WINDOW_WIDTH,
      height: WINDOW_HEIGHT,
      show: false,
      frame: false,
      transparent: true,
      alwaysOnTop: true,
      skipTaskbar: true,
      focusable: isMac ? false : true,
      hasShadow: false,
      resizable: false,
      movable: false,
      minimizable: false,
      maximizable: false,
      closable: false,
      ...(isMac ? { type: 'panel' as const } : {}),
      enableLargerThanScreen: true,
      webPreferences: {
        preload: join(__dirname, '../preload/notch.js'),
        sandbox: true,
        backgroundThrottling: false,
      },
    })

    this.overlayWindow.setAlwaysOnTop(true, 'pop-up-menu')
    if (isMac) {
      this.overlayWindow.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true })
    }
    this.overlayWindow.setIgnoreMouseEvents(true, { forward: true })

    // On Windows the overlay must be focusable for clicks to register,
    // but we don't want it to steal focus from the main window.
    if (!isMac) {
      this.overlayWindow.on('focus', () => {
        this.overlayWindow?.blur()
      })
    }

    const ready =
      is.dev && process.env['ELECTRON_RENDERER_URL']
        ? this.overlayWindow.loadURL(`${process.env['ELECTRON_RENDERER_URL']}/notch.html`)
        : this.overlayWindow.loadFile(join(__dirname, '../renderer/notch.html'))

    // Warm up the renderer on Windows so the first hover doesn't stutter.
    // Briefly show the (empty) overlay off-screen to force GPU compositing.
    if (process.platform === 'win32') {
      ready.then(() => {
        if (!this.overlayWindow || this.overlayWindow.isDestroyed()) return
        this.overlayWindow.setPosition(-WINDOW_WIDTH, 0)
        this.overlayWindow.showInactive()
        setTimeout(() => {
          if (!this.overlayWindow || this.overlayWindow.isDestroyed()) return
          this.overlayWindow.hide()
          this.repositionWindow()
        }, 200)
      })
    }
  }

  private bindAgentEvents(): void {
    this.statusChangeHandler = (status: NotchSessionStatus) => {
      const prev = this.previousStatuses.get(status.ptySessionId)

      if (status.status === 'ended') {
        this.sessions.delete(status.ptySessionId)
        this.previousStatuses.delete(status.ptySessionId)
      } else {
        // Detect peek-worthy transitions before updating
        if (this.isPeekWorthy(prev, status.status)) {
          if (!this.isSessionVisibleInFocusedWindow(status)) {
            this.pendingPeekIds.add(status.ptySessionId)
          }
        }
        this.previousStatuses.set(status.ptySessionId, status.status)
        this.sessions.set(status.ptySessionId, status)
      }
      this.pushState()
    }

    this.sessionDestroyedHandler = (ptySessionId: string) => {
      this.previousStatuses.delete(ptySessionId)
      if (this.sessions.delete(ptySessionId)) {
        this.pushState()
      }
    }

    this.agentSessionManager.on('statusChange', this.statusChangeHandler)
    this.agentSessionManager.on('sessionDestroyed', this.sessionDestroyedHandler)
  }

  /** Check if the session is the focused agent pane in a focused window */
  private isSessionVisibleInFocusedWindow(status: NotchSessionStatus): boolean {
    const win = this.windowManager.getWindowById(status.windowId)
    if (!win || !win.isFocused()) return false
    return this.windowManager.getFocusedAgentSession(status.windowId) === status.ptySessionId
  }

  private isPeekWorthy(prev: SessionStatusType | undefined, next: SessionStatusType): boolean {
    if (next === 'waitingPermission' || next === 'error') return true
    if (next === 'idle' && prev !== undefined && prev !== 'idle' && prev !== 'ended') {
      return true
    }
    return false
  }

  private bindIpcHandlers(): void {
    ipcMain.handle(
      'notch:focusSession',
      (event, { windowId, ptySessionId }: { windowId: number; ptySessionId: string }) => {
        if (!this.overlayWindow || this.overlayWindow.isDestroyed()) return
        if (event.sender !== this.overlayWindow.webContents) return
        const win = this.windowManager.getWindowById(windowId)
        if (!win) return

        if (win.isMinimized()) win.restore()
        win.focus()
        win.webContents.send('agent:focusSession', { ptySessionId })
      },
    )

    ipcMain.on('notch:setMouseIgnore', (event, { ignore }: { ignore: boolean }) => {
      if (!this.overlayWindow || this.overlayWindow.isDestroyed()) return
      if (event.sender !== this.overlayWindow.webContents) return
      if (ignore) {
        this.overlayWindow.setIgnoreMouseEvents(true, { forward: true })
      } else {
        this.overlayWindow.setIgnoreMouseEvents(false)
      }
    })
  }

  private watchDisplayChanges(): void {
    this.displayChangeHandler = () => {
      if (!this.hasNotch()) {
        if (this.overlayWindow && !this.overlayWindow.isDestroyed()) {
          this.overlayWindow.hide()
        }
        return
      }
      this.repositionWindow()
    }
    screen.on('display-metrics-changed', this.displayChangeHandler)
  }

  private repositionWindow(): void {
    if (!this.overlayWindow || this.overlayWindow.isDestroyed()) return
    const primary = screen.getPrimaryDisplay()
    const x = Math.round(primary.workArea.x + primary.workArea.width / 2 - WINDOW_WIDTH / 2)
    const y = 0
    this.overlayWindow.setPosition(x, y)
  }

  private pushState(): void {
    if (!this.overlayWindow || this.overlayWindow.isDestroyed()) return

    const notch = this.getNotchDimensions()
    const peekSessionIds = this.pendingPeekIds.size > 0 ? [...this.pendingPeekIds] : undefined
    this.pendingPeekIds.clear()

    const state: NotchOverlayState = {
      sessions: [...this.sessions.values()],
      notchWidth: notch.width,
      notchHeight: notch.height,
      peekSessionIds,
    }

    if (state.sessions.length > 0) {
      this.overlayWindow.showInactive()
    } else {
      this.overlayWindow.hide()
    }

    this.overlayWindow.webContents.send('notch:stateUpdate', state)
  }
}
