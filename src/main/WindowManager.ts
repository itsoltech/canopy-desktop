import { app, BrowserWindow, dialog, screen, shell } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'
import icon from '../../resources/icon.png?asset'
import type { PtyManager } from './pty/PtyManager'
import type { GitWatcher } from './git/GitWatcher'
import type { FileTreeWatcher } from './fileWatcher/FileTreeWatcher'
import type { AgentSessionManager } from './agents/AgentSessionManager'
import type { BrowserManager } from './browser/BrowserManager'
import type { TerminalStreamService } from './pty/TerminalStreamService'
import { TmuxManager } from './pty/TmuxManager'
import { isSafeExternalUrl } from './security/validateUrl'
import { comparableWorkspacePath, normalizeWorkspacePath } from './db/workspacePaths'
import type { WindowBounds, WindowConfig, WindowState } from './windowBounds'

interface CreateWindowOptions {
  bounds?: WindowBounds
  windowState?: WindowState
  startupRestore?: boolean
}

interface WindowCloseSnapshot {
  configs: WindowConfig[]
  isLastWindow: boolean
}

export class WindowManager {
  private windows = new Map<number, BrowserWindow>()
  // Keyed by comparable form (separator- and, on win32, case-folded) so lookups can
  // never miss on spelling; values keep the display form for configs and callbacks.
  private workspacePaths = new Map<number, Map<string, string>>()
  private activeWorktreePaths = new Map<number, string>()
  private startupRestoreWindows = new Map<number, boolean>()
  private gitWatchers = new Map<number, Map<string, GitWatcher>>()
  private fileWatchers = new Map<number, FileTreeWatcher>()
  private ptySessions = new Map<number, Set<string>>()
  private forceClosing = new Set<number>()
  private focusedAgentSessions = new Map<number, string>()
  private agentSessionManager: AgentSessionManager | null = null
  private browserManager: BrowserManager | null = null
  private terminalStreamService: TerminalStreamService | null = null
  private tmuxManager: TmuxManager | null = null
  private allWindowsClosedCallback: (() => void) | null = null
  private windowDisposeCallback: ((paths: string[]) => void) | null = null
  private windowCloseSnapshotCallback: ((snapshot: WindowCloseSnapshot) => void) | null = null
  private windowConfigChangedCallback: ((configs: WindowConfig[]) => void) | null = null

  private ptyManager: PtyManager
  isQuitting = false

  constructor(ptyManager: PtyManager) {
    this.ptyManager = ptyManager
  }

  setAgentSessionManager(asm: AgentSessionManager): void {
    this.agentSessionManager = asm
  }

  setBrowserManager(bm: BrowserManager): void {
    this.browserManager = bm
  }

  setTerminalStreamService(service: TerminalStreamService): void {
    this.terminalStreamService = service
  }

  setTmuxManager(tm: TmuxManager): void {
    this.tmuxManager = tm
  }

  setOnWindowDispose(cb: (paths: string[]) => void): void {
    this.windowDisposeCallback = cb
  }

  setOnWindowCloseSnapshot(cb: (snapshot: WindowCloseSnapshot) => void): void {
    this.windowCloseSnapshotCallback = cb
  }

  setOnWindowConfigChanged(cb: (configs: WindowConfig[]) => void): void {
    this.windowConfigChangedCallback = cb
  }

  createWindow(options?: CreateWindowOptions): BrowserWindow {
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
        // SECURITY: Electron 41 defaults are secure, but set explicitly as defense-in-depth
        // so renderer cannot access Node.js APIs even if defaults change.
        nodeIntegration: false,
        contextIsolation: true,
      },
    })

    const wcId = win.webContents.id
    this.windows.set(wcId, win)
    this.ptySessions.set(wcId, new Set())
    this.startupRestoreWindows.set(wcId, options?.startupRestore === true)

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
        this.captureWindowCloseSnapshot()
        return
      }

      // Fast sync check: nothing tracked → allow the close to proceed without
      // spawning helper processes. Only when this window owns PTY sessions do
      // we preventDefault and run the (async) busy-check + confirmation dialog.
      if (!this.hasTrackedPtySessions(wcId)) {
        this.captureWindowCloseSnapshot()
        return
      }

      event.preventDefault()

      void this.getActiveSessionInfo(wcId).then(async (detail) => {
        if (!detail) {
          this.forceClosing.add(wcId)
          win.close()
          return
        }
        const { response } = await dialog.showMessageBox(win, {
          type: 'warning',
          buttons: ['Close Window', 'Cancel'],
          defaultId: 1,
          cancelId: 1,
          title: 'Active Sessions',
          message: 'This window has active sessions',
          detail,
        })
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

    // The main renderer is a bundled single-page app and must never navigate
    // away from its own document. Block any top-level navigation — defense in
    // depth against an injected window.location change escaping this privileged,
    // IPC-capable context — and route safe external URLs to the OS browser,
    // mirroring the window-open handler above. `will-navigate` does not fire on
    // reloads or History API changes, so legitimate in-app behaviour is
    // unaffected.
    win.webContents.on('will-navigate', (event, url) => {
      if (url === win.webContents.getURL()) return
      event.preventDefault()
      if (isSafeExternalUrl(url)) shell.openExternal(url)
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

  // Tracked paths are normalized on the way in and matched by comparable form, so
  // lookups, persisted window configs, and workspace-store rows can never diverge
  // again on separator style or (win32) letter case — the source of un-deletable
  // ghost windows.
  getWindowForPath(path: string): BrowserWindow | null {
    const key = comparableWorkspacePath(path)
    for (const [wcId, paths] of this.workspacePaths) {
      const active = this.activeWorktreePaths.get(wcId)
      if (paths.has(key) || (active && comparableWorkspacePath(active) === key)) {
        const win = this.windows.get(wcId)
        if (win && !win.isDestroyed()) return win
      }
    }
    return null
  }

  addWorkspacePath(wcId: number, path: string): void {
    let paths = this.workspacePaths.get(wcId)
    if (!paths) {
      paths = new Map()
      this.workspacePaths.set(wcId, paths)
    }
    paths.set(comparableWorkspacePath(path), normalizeWorkspacePath(path))
  }

  removeWorkspacePath(wcId: number, path: string): void {
    const paths = this.workspacePaths.get(wcId)
    if (paths) paths.delete(comparableWorkspacePath(path))
  }

  setActiveWorktree(wcId: number, path: string): void {
    this.activeWorktreePaths.set(wcId, normalizeWorkspacePath(path))
  }

  clearActiveWorktree(wcId: number): void {
    this.activeWorktreePaths.delete(wcId)
  }

  shouldQuitOnLastWindowClose(): boolean {
    return process.platform !== 'darwin' || process.env.CANOPY_E2E_CLOSE_LAST_WINDOW_QUITS === '1'
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
    const result = paths ? [...paths.values()] : []
    const active = this.activeWorktreePaths.get(wcId)
    if (
      active &&
      !result.some((p) => comparableWorkspacePath(p) === comparableWorkspacePath(active))
    ) {
      result.push(active)
    }
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
          paths: [...paths.values()],
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

  ownsPtySession(wcId: number, sessionId: string): boolean {
    return this.ptySessions.get(wcId)?.has(sessionId) ?? false
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
      void watcher.stop()
      watchers.delete(repoRoot)
    }
  }

  disposeAllGitWatchers(wcId: number): void {
    const watchers = this.gitWatchers.get(wcId)
    if (!watchers) return
    for (const watcher of watchers.values()) {
      void watcher.stop()
    }
    watchers.clear()
  }

  /**
   * Stop every watcher (any window) holding native directory handles inside
   * `dirPath` and wait for the unsubscribes to complete. On Windows a live
   * ReadDirectoryChangesW handle blocks directory deletion, so worktree removal
   * must tear these down BEFORE `git worktree remove` — the renderer re-arms its
   * watchers when the selection settles afterwards.
   */
  async disposeWatchersUnderPathAndWait(dirPath: string): Promise<void> {
    const base = comparableWorkspacePath(dirPath).replace(/\/+$/, '')
    const prefix = `${base}/`
    const inside = (p: string): boolean => {
      const c = comparableWorkspacePath(p)
      return c === base || c.startsWith(prefix)
    }

    const waits: Promise<unknown>[] = []
    for (const watchers of this.gitWatchers.values()) {
      for (const [key, gitWatcher] of [...watchers]) {
        if (inside(gitWatcher.root)) {
          waits.push(gitWatcher.stop())
          watchers.delete(key)
        }
      }
    }
    for (const [wcId, fileWatcher] of [...this.fileWatchers]) {
      if (inside(fileWatcher.root)) {
        waits.push(fileWatcher.stop().unwrapOr(undefined))
        this.fileWatchers.delete(wcId)
      }
    }
    await Promise.all(waits)
  }

  setFileWatcher(wcId: number, watcher: FileTreeWatcher): void {
    this.fileWatchers.set(wcId, watcher)
  }

  getFileWatcher(wcId: number): FileTreeWatcher | null {
    return this.fileWatchers.get(wcId) ?? null
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

  hasStartupRestore(wcId: number): boolean {
    return this.startupRestoreWindows.get(wcId) ?? false
  }

  completeStartupRestore(wcId: number): void {
    this.startupRestoreWindows.set(wcId, false)
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

  /** True when this window owns at least one PTY session — cheap sync check. */
  hasTrackedPtySessions(wcId: number): boolean {
    const sessionIds = this.ptySessions.get(wcId)
    return !!sessionIds && sessionIds.size > 0
  }

  async getActiveSessionInfo(wcId: number): Promise<string | null> {
    const sessionIds = this.ptySessions.get(wcId)
    if (!sessionIds || sessionIds.size === 0) return null

    let busyAgentCount = 0
    let activeShellCount = 0

    const shellChecks: Promise<boolean>[] = []
    for (const sid of sessionIds) {
      if (this.agentSessionManager?.isAgentSession(sid)) {
        if (this.agentSessionManager.isBusy(sid)) {
          busyAgentCount++
        }
      } else {
        shellChecks.push(this.ptyManager.hasChildProcess(sid))
      }
    }
    const shellResults = await Promise.all(shellChecks)
    for (const has of shellResults) {
      if (has) activeShellCount++
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

  async hasAnyActiveSession(): Promise<string | null> {
    for (const [wcId] of this.windows) {
      const info = await this.getActiveSessionInfo(wcId)
      if (info) return info
    }
    return null
  }

  private disposeWindow(wcId: number): void {
    const closesApplication =
      !this.isQuitting && this.shouldQuitOnLastWindowClose() && this.windows.size === 1
    const shouldPersistWindowConfigs = !this.isQuitting && !closesApplication

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
        const tmuxName = this.ptyManager.getTmuxSessionName(sid)
        this.terminalStreamService?.destroy(sid)
        if (!this.isQuitting && this.tmuxManager) {
          if (tmuxName && TmuxManager.isCanopySession(tmuxName)) {
            this.tmuxManager.killSession(tmuxName).catch(() => {})
          }
        }
        this.ptyManager.kill(sid, { killProcessTree: this.isQuitting })
        // Release agent-session bookkeeping (hook-router entry, generated
        // settings files, busy/session maps) on window close — not only on tab
        // close or app quit — otherwise an agent tab still open when its window
        // closes leaks its session. No-op for non-agent PTY sessions.
        this.agentSessionManager?.destroySession(sid)
      }
    }

    // Delete workspace layouts when a non-quitting window is manually closed.
    // Closing the last window quits the app on Windows/Linux, so keep layouts
    // for startup restore in that path.
    if (!this.isQuitting && !closesApplication && this.windowDisposeCallback) {
      const paths = this.workspacePaths.get(wcId)
      if (paths && paths.size > 0) {
        this.windowDisposeCallback([...paths.values()])
      }
    }

    this.windows.delete(wcId)
    this.workspacePaths.delete(wcId)
    this.activeWorktreePaths.delete(wcId)
    this.startupRestoreWindows.delete(wcId)
    this.focusedAgentSessions.delete(wcId)
    this.ptySessions.delete(wcId)

    if (shouldPersistWindowConfigs && this.windowConfigChangedCallback) {
      this.windowConfigChangedCallback(this.getAllWindowConfigs())
    }
  }

  private captureWindowCloseSnapshot(): void {
    if (this.isQuitting || !this.windowCloseSnapshotCallback) return

    const configs = this.getAllWindowConfigs()
    if (configs.length === 0) return

    this.windowCloseSnapshotCallback({
      configs,
      isLastWindow: this.windows.size === 1,
    })
  }

  disposeAll(): void {
    for (const wcId of [...this.windows.keys()]) {
      this.disposeWindow(wcId)
    }
  }
}
