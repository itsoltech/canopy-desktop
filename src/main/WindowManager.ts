import { BrowserWindow, dialog, screen, shell } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'
import icon from '../../resources/icon.png?asset'
import type { PtyManager } from './pty/PtyManager'
import type { WsBridge } from './pty/WsBridge'
import type { GitWatcher } from './git/GitWatcher'
import type { ClaudeSessionManager } from './claude/ClaudeSessionManager'

export class WindowManager {
  private windows = new Map<number, BrowserWindow>()
  private workspacePaths = new Map<number, string>()
  private gitWatchers = new Map<number, GitWatcher>()
  private ptySessions = new Map<number, Set<string>>()
  private forceClosing = new Set<number>()
  private claudeSessionManager: ClaudeSessionManager | null = null

  private ptyManager: PtyManager
  private wsBridge: WsBridge
  isQuitting = false

  constructor(ptyManager: PtyManager, wsBridge: WsBridge) {
    this.ptyManager = ptyManager
    this.wsBridge = wsBridge
  }

  setClaudeSessionManager(csm: ClaudeSessionManager): void {
    this.claudeSessionManager = csm
  }

  createWindow(): BrowserWindow {
    const win = new BrowserWindow({
      width: 1200,
      height: 800,
      minWidth: 600,
      minHeight: 400,
      show: false,
      autoHideMenuBar: true,
      transparent: false,
      titleBarStyle: 'hidden',
      trafficLightPosition: { x: 12, y: 12 },
      vibrancy: 'under-window',
      backgroundColor: '#333',
      ...(process.platform !== 'darwin'
        ? { titleBarOverlay: { color: '#00000000', symbolColor: '#e0e0e0', height: 40 } }
        : {}),
      ...(process.platform === 'linux' ? { icon } : {}),
      webPreferences: {
        preload: join(__dirname, '../preload/index.js'),
        sandbox: false,
      },
    })

    const wcId = win.webContents.id
    this.windows.set(wcId, win)
    this.ptySessions.set(wcId, new Set())

    // Force re-render when window moves between displays with different scale factors
    let lastScaleFactor = screen.getDisplayMatching(win.getBounds()).scaleFactor
    win.on('moved', () => {
      const currentScale = screen.getDisplayMatching(win.getBounds()).scaleFactor
      if (currentScale !== lastScaleFactor) {
        lastScaleFactor = currentScale
        win.webContents.invalidate()
      }
    })

    win.on('ready-to-show', () => {
      win.show()
    })

    win.on('close', (event) => {
      if (this.isQuitting || this.forceClosing.has(wcId)) {
        this.forceClosing.delete(wcId)
        return
      }

      const detail = this.getActiveSessionInfo(wcId)
      if (!detail) return

      event.preventDefault()

      dialog
        .showMessageBox(win, {
          type: 'warning',
          buttons: ['Close Window', 'Cancel'],
          defaultId: 1,
          cancelId: 1,
          title: 'Active Sessions',
          message: 'This window has active sessions',
          detail,
        })
        .then(({ response }) => {
          if (response === 0) {
            this.forceClosing.add(wcId)
            win.close()
          }
        })
    })

    win.on('closed', () => {
      this.disposeWindow(wcId)
    })

    win.webContents.setWindowOpenHandler((details) => {
      shell.openExternal(details.url)
      return { action: 'deny' }
    })

    if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
      win.loadURL(process.env['ELECTRON_RENDERER_URL'])
    } else {
      win.loadFile(join(__dirname, '../renderer/index.html'))
    }

    return win
  }

  getWindowForPath(path: string): BrowserWindow | null {
    for (const [wcId, wsPath] of this.workspacePaths) {
      if (wsPath === path) {
        const win = this.windows.get(wcId)
        if (win && !win.isDestroyed()) return win
      }
    }
    return null
  }

  setWorkspacePath(wcId: number, path: string): void {
    this.workspacePaths.set(wcId, path)
  }

  getWorkspacePath(wcId: number): string | null {
    return this.workspacePaths.get(wcId) ?? null
  }

  getAllWorkspacePaths(): string[] {
    const paths: string[] = []
    for (const [wcId, path] of this.workspacePaths) {
      const win = this.windows.get(wcId)
      if (win && !win.isDestroyed()) paths.push(path)
    }
    return paths
  }

  trackPtySession(wcId: number, sessionId: string): void {
    const set = this.ptySessions.get(wcId)
    if (set) set.add(sessionId)
  }

  untrackPtySession(wcId: number, sessionId: string): void {
    const set = this.ptySessions.get(wcId)
    if (set) set.delete(sessionId)
  }

  setGitWatcher(wcId: number, watcher: GitWatcher): void {
    this.gitWatchers.set(wcId, watcher)
  }

  getGitWatcher(wcId: number): GitWatcher | null {
    return this.gitWatchers.get(wcId) ?? null
  }

  disposeGitWatcher(wcId: number): void {
    const watcher = this.gitWatchers.get(wcId)
    if (watcher) {
      watcher.stop()
      this.gitWatchers.delete(wcId)
    }
  }

  getAllWindows(): BrowserWindow[] {
    const result: BrowserWindow[] = []
    for (const win of this.windows.values()) {
      if (!win.isDestroyed()) result.push(win)
    }
    return result
  }

  get size(): number {
    return this.windows.size
  }

  getActiveSessionInfo(wcId: number): string | null {
    const sessionIds = this.ptySessions.get(wcId)
    if (!sessionIds || sessionIds.size === 0) return null

    let busyClaudeCount = 0
    let activeShellCount = 0

    for (const sid of sessionIds) {
      if (this.claudeSessionManager?.isClaudeSession(sid)) {
        if (this.claudeSessionManager.isBusy(sid)) {
          busyClaudeCount++
        }
      } else {
        if (this.ptyManager.hasChildProcess(sid)) {
          activeShellCount++
        }
      }
    }

    if (busyClaudeCount === 0 && activeShellCount === 0) return null

    const parts: string[] = []
    if (busyClaudeCount > 0) {
      parts.push(`${busyClaudeCount} active Claude session${busyClaudeCount > 1 ? 's' : ''}`)
    }
    if (activeShellCount > 0) {
      parts.push(`${activeShellCount} running process${activeShellCount > 1 ? 'es' : ''}`)
    }
    return parts.join(' and ') + ' will be terminated.'
  }

  hasAnyActiveSession(): string | null {
    for (const [wcId] of this.windows) {
      const info = this.getActiveSessionInfo(wcId)
      if (info) return info
    }
    return null
  }

  private disposeWindow(wcId: number): void {
    // Stop git watcher
    this.disposeGitWatcher(wcId)

    // Kill PTY sessions for this window
    const sessions = this.ptySessions.get(wcId)
    if (sessions) {
      for (const sid of sessions) {
        this.wsBridge.destroy(sid)
        this.ptyManager.kill(sid)
      }
    }

    this.windows.delete(wcId)
    this.workspacePaths.delete(wcId)
    this.ptySessions.delete(wcId)
  }

  disposeAll(): void {
    for (const wcId of [...this.windows.keys()]) {
      this.disposeWindow(wcId)
    }
  }
}
