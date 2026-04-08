import { app, BrowserWindow, dialog, screen, shell } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'
import icon from '../../resources/icon.png?asset'
import type { PtyManager } from './pty/PtyManager'
import type { WsBridge } from './pty/WsBridge'
import type { GitWatcher } from './git/GitWatcher'
import type { FileTreeWatcher } from './fileWatcher/FileTreeWatcher'
import type { AgentSessionManager } from './agents/AgentSessionManager'
import type { BrowserManager } from './browser/BrowserManager'
import { TmuxManager } from './pty/TmuxManager'
import { isSafeExternalUrl } from './security/validateUrl'
import type { WindowBounds, WindowConfig, WindowState } from './windowBounds'

export class WindowManager {
  private windows = new Map<number, BrowserWindow>()
  private workspacePaths = new Map<number, Set<string>>()
  private activeWorktreePaths = new Map<number, string>()
  private gitWatchers = new Map<number, Map<string, GitWatcher>>()
  private fileWatchers = new Map<number, FileTreeWatcher>()
  private ptySessions = new Map<number, Set<string>>()
  private forceClosing = new Set<number>()
  private focusedAgentSessions = new Map<number, string>()
  private agentSessionManager: AgentSessionManager | null = null
  private browserManager: BrowserManager | null = null
  private tmuxManager: TmuxManager | null = null
  private allWindowsClosedCallback: (() => void) | null = null
  private windowDisposeCallback: ((paths: string[]) => void) | null = null

  private ptyManager: PtyManager
  private wsBridge: WsBridge
  isQuitting = false

  constructor(ptyManager: PtyManager, wsBridge: WsBridge) {
    this.ptyManager = ptyManager
    this.wsBridge = wsBridge
  }

  setAgentSessionManager(asm: AgentSessionManager): void {
    this.agentSessionManager = asm
  }

  setBrowserManager(bm: BrowserManager): void {
    this.browserManager = bm
  }

  setTmuxManager(tm: TmuxManager): void {
    this.tmuxManager = tm
  }

  setOnWindowDispose(cb: (paths: string[]) => void): void {
    this.windowDisposeCallback = cb
  }

  createWindow(options?: { bounds?: WindowBounds; windowState?: WindowState }): BrowserWindow {
    const sizeDefaults = { width: 1200, height: 800 }
    const boundsOpts = options?.bounds
      ? {
          x: options.bounds.x,
          y: options.bounds.y,
          width: options.bounds.width,
          height: options.bounds.height,
        }
      : sizeDefaults

    const win = new BrowserWindow({
      ...boundsOpts,
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
        // SECURITY: sandbox disabled — required for node-pty preload bridge.
        // Browser <webview> tags use sandbox: true via webpreferences attribute.
        sandbox: false,
        webviewTag: true,
      },
    })

    const wcId = win.webContents.id
    this.windows.set(wcId, win)
    this.ptySessions.set(wcId, new Set())

    // Track webview guest webContents for keyboard interception + DevTools
    if (this.browserManager) {
      this.browserManager.trackWindow(win)
    }

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
      if (options?.windowState === 'maximized') win.maximize()
      else if (options?.windowState === 'fullscreen') win.setFullScreen(true)
      if (!process.env.CANOPY_E2E || app.isPackaged) win.show()
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
      // When the last managed window closes, destroy notch overlay
      // before Electron checks window count for window-all-closed.
      if (this.windows.size === 0 && this.allWindowsClosedCallback) {
        this.allWindowsClosedCallback()
      }
    })

    win.webContents.setWindowOpenHandler((details) => {
      if (isSafeExternalUrl(details.url)) shell.openExternal(details.url)
      return { action: 'deny' }
    })

    if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
      win.loadURL(process.env['ELECTRON_RENDERER_URL'])
    } else {
      win.loadFile(join(__dirname, '../renderer/index.html'))
    }

    return win
  }

  get gitWatcherCount(): number {
    let count = 0
    for (const watchers of this.gitWatchers.values()) {
      count += watchers.size
    }
    return count
  }

  getWindowForPath(path: string): BrowserWindow | null {
    for (const [wcId, paths] of this.workspacePaths) {
      if (paths.has(path)) {
        const win = this.windows.get(wcId)
        if (win && !win.isDestroyed()) return win
      }
    }
    return null
  }

  addWorkspacePath(wcId: number, path: string): void {
    let paths = this.workspacePaths.get(wcId)
    if (!paths) {
      paths = new Set()
      this.workspacePaths.set(wcId, paths)
    }
    paths.add(path)
  }

  removeWorkspacePath(wcId: number, path: string): void {
    const paths = this.workspacePaths.get(wcId)
    if (paths) paths.delete(path)
  }

  setActiveWorktree(wcId: number, path: string): void {
    this.activeWorktreePaths.set(wcId, path)
  }

  setFocusedAgentSession(wcId: number, ptySessionId: string | null): void {
    if (ptySessionId) {
      this.focusedAgentSessions.set(wcId, ptySessionId)
    } else {
      this.focusedAgentSessions.delete(wcId)
    }
  }

  getFocusedAgentSession(wcId: number): string | null {
    return this.focusedAgentSessions.get(wcId) ?? null
  }

  getWorkspacePaths(wcId: number): string[] {
    const paths = this.workspacePaths.get(wcId)
    const result = paths ? [...paths] : []
    const active = this.activeWorktreePaths.get(wcId)
    if (active && !result.includes(active)) result.push(active)
    return result
  }

  /** Returns one entry per window, each containing all project paths for that window */
  getAllWindowConfigs(): WindowConfig[] {
    const configs: WindowConfig[] = []
    for (const [wcId, paths] of this.workspacePaths) {
      const win = this.windows.get(wcId)
      if (win && !win.isDestroyed() && paths.size > 0) {
        const isMax = win.isMaximized()
        const isFs = win.isFullScreen()
        const bounds = isMax || isFs ? win.getNormalBounds() : win.getBounds()
        const windowState: WindowState = isFs ? 'fullscreen' : isMax ? 'maximized' : 'normal'

        configs.push({
          paths: [...paths],
          activeWorktreePath: this.activeWorktreePaths.get(wcId),
          bounds,
          windowState,
        })
      }
    }
    return configs
  }

  trackPtySession(wcId: number, sessionId: string): void {
    const set = this.ptySessions.get(wcId)
    if (set) set.add(sessionId)
  }

  untrackPtySession(wcId: number, sessionId: string): void {
    const set = this.ptySessions.get(wcId)
    if (set) set.delete(sessionId)
  }

  setGitWatcher(wcId: number, repoRoot: string, watcher: GitWatcher): void {
    let watchers = this.gitWatchers.get(wcId)
    if (!watchers) {
      watchers = new Map()
      this.gitWatchers.set(wcId, watchers)
    }
    watchers.set(repoRoot, watcher)
  }

  getGitWatcher(wcId: number, repoRoot: string): GitWatcher | null {
    return this.gitWatchers.get(wcId)?.get(repoRoot) ?? null
  }

  disposeGitWatcher(wcId: number, repoRoot: string): void {
    const watchers = this.gitWatchers.get(wcId)
    if (!watchers) return
    const watcher = watchers.get(repoRoot)
    if (watcher) {
      watcher.stop()
      watchers.delete(repoRoot)
    }
  }

  disposeAllGitWatchers(wcId: number): void {
    const watchers = this.gitWatchers.get(wcId)
    if (!watchers) return
    for (const watcher of watchers.values()) {
      watcher.stop()
    }
    watchers.clear()
  }

  setFileWatcher(wcId: number, watcher: FileTreeWatcher): void {
    this.fileWatchers.set(wcId, watcher)
  }

  getFileWatcher(wcId: number): FileTreeWatcher | null {
    return this.fileWatchers.get(wcId) ?? null
  }

  getAllFileWatchers(): FileTreeWatcher[] {
    return [...this.fileWatchers.values()]
  }

  disposeFileWatcher(wcId: number): void {
    const watcher = this.fileWatchers.get(wcId)
    if (watcher) {
      void watcher.stop()
      this.fileWatchers.delete(wcId)
    }
  }

  getWindowById(wcId: number): BrowserWindow | null {
    const win = this.windows.get(wcId)
    return win && !win.isDestroyed() ? win : null
  }

  getAllWindows(): BrowserWindow[] {
    const result: BrowserWindow[] = []
    for (const win of this.windows.values()) {
      if (!win.isDestroyed()) result.push(win)
    }
    return result
  }

  getLastFocusedBounds(): WindowBounds | null {
    const focused = BrowserWindow.getFocusedWindow()
    if (focused && !focused.isDestroyed()) return focused.getBounds()
    const allWins = this.getAllWindows()
    if (allWins.length > 0) return allWins[allWins.length - 1].getBounds()
    return null
  }

  onAllWindowsClosed(callback: () => void): void {
    this.allWindowsClosedCallback = callback
  }

  get size(): number {
    return this.windows.size
  }

  getActiveSessionInfo(wcId: number): string | null {
    const sessionIds = this.ptySessions.get(wcId)
    if (!sessionIds || sessionIds.size === 0) return null

    let busyAgentCount = 0
    let activeShellCount = 0

    for (const sid of sessionIds) {
      if (this.agentSessionManager?.isAgentSession(sid)) {
        if (this.agentSessionManager.isBusy(sid)) {
          busyAgentCount++
        }
      } else {
        if (this.ptyManager.hasChildProcess(sid)) {
          activeShellCount++
        }
      }
    }

    if (busyAgentCount === 0 && activeShellCount === 0) return null

    const parts: string[] = []
    if (busyAgentCount > 0) {
      parts.push(`${busyAgentCount} active AI session${busyAgentCount > 1 ? 's' : ''}`)
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
    // Stop all git watchers for this window
    this.disposeAllGitWatchers(wcId)
    this.gitWatchers.delete(wcId)

    // Stop file tree watcher for this window
    this.disposeFileWatcher(wcId)

    // Teardown browser webviews owned by this window
    const win = this.windows.get(wcId)
    if (win && this.browserManager) {
      this.browserManager.teardownAllForWindow(win)
    }

    // Kill PTY sessions (and their tmux sessions) for this window
    const sessions = this.ptySessions.get(wcId)
    if (sessions) {
      for (const sid of sessions) {
        if (!this.isQuitting && this.tmuxManager) {
          const tmuxName = this.ptyManager.getTmuxSessionName(sid)
          if (tmuxName && TmuxManager.isCanopySession(tmuxName)) {
            this.tmuxManager.killSession(tmuxName).catch(() => {})
          }
        }
        this.wsBridge.destroy(sid)
        this.ptyManager.kill(sid)
      }
    }

    // Delete workspace layouts when window is manually closed (not during quit)
    if (!this.isQuitting && this.windowDisposeCallback) {
      const paths = this.workspacePaths.get(wcId)
      if (paths && paths.size > 0) {
        this.windowDisposeCallback([...paths])
      }
    }

    this.windows.delete(wcId)
    this.workspacePaths.delete(wcId)
    this.activeWorktreePaths.delete(wcId)
    this.focusedAgentSessions.delete(wcId)
    this.ptySessions.delete(wcId)
  }

  disposeAll(): void {
    for (const wcId of [...this.windows.keys()]) {
      this.disposeWindow(wcId)
    }
  }
}
