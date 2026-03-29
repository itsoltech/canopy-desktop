import { app, BrowserWindow, dialog, screen, shell } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'
import icon from '../../resources/icon.png?asset'
import type { PtyManager } from './pty/PtyManager'
import type { WsBridge } from './pty/WsBridge'
import type { GitWatcher } from './git/GitWatcher'
import type { ClaudeSessionManager } from './claude/ClaudeSessionManager'
import { isSafeExternalUrl } from './security/validateUrl'

export class WindowManager {
  private windows = new Map<number, BrowserWindow>()
  private workspacePaths = new Map<number, Set<string>>()
  private activeWorktreePaths = new Map<number, string>()
  private gitWatchers = new Map<number, Map<string, GitWatcher>>()
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
        // SECURITY: sandbox disabled — required for node-pty preload bridge.
        // Browser WebContentsViews use sandbox: true separately.
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

  getWorkspacePaths(wcId: number): string[] {
    const paths = this.workspacePaths.get(wcId)
    return paths ? [...paths] : []
  }

  /** Returns one entry per window, each containing all project paths for that window */
  getAllWindowConfigs(): Array<{ paths: string[]; activeWorktreePath?: string }> {
    const configs: Array<{ paths: string[]; activeWorktreePath?: string }> = []
    for (const [wcId, paths] of this.workspacePaths) {
      const win = this.windows.get(wcId)
      if (win && !win.isDestroyed() && paths.size > 0) {
        configs.push({
          paths: [...paths],
          activeWorktreePath: this.activeWorktreePaths.get(wcId),
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
    // Stop all git watchers for this window
    this.disposeAllGitWatchers(wcId)
    this.gitWatchers.delete(wcId)

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
