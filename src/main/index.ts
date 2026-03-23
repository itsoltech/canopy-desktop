import { app, shell, BrowserWindow } from 'electron'
import { join, resolve } from 'path'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'
import icon from '../../resources/icon.png?asset'
import { PtyManager } from './pty/PtyManager'
import { WsBridge } from './pty/WsBridge'
import { Database } from './db/Database'
import { WorkspaceStore } from './db/WorkspaceStore'
import { PreferencesStore } from './db/PreferencesStore'
import { LayoutStore } from './db/LayoutStore'
import { ToolRegistry } from './tools/ToolRegistry'
import { registerIpcHandlers, disposeGitWatcher } from './ipc/handlers'
import { ClaudeSessionManager } from './claude/ClaudeSessionManager'
import { resolveLoginEnv } from './shell/loginEnv'

const ptyManager = new PtyManager()
const wsBridge = new WsBridge()
const database = new Database()
const workspaceStore = new WorkspaceStore(database)
const preferencesStore = new PreferencesStore(database)
const layoutStore = new LayoutStore(database)
const toolRegistry = new ToolRegistry(database)

let mainWindow: BrowserWindow | null = null
let claudeSessionManager: ClaudeSessionManager | null = null

// Register nixtty:// URL scheme
if (process.defaultApp) {
  if (process.argv.length >= 2) {
    app.setAsDefaultProtocolClient('nixtty', process.execPath, [resolve(process.argv[1])])
  }
} else {
  app.setAsDefaultProtocolClient('nixtty')
}

// Ensure single instance (required for URL scheme on Windows/Linux)
const gotTheLock = app.requestSingleInstanceLock()
if (!gotTheLock) {
  app.quit()
}

function handleNixttyUrl(url: string): void {
  if (!mainWindow || mainWindow.isDestroyed()) return

  try {
    const parsed = new URL(url)
    const path = parsed.searchParams.get('path')
    if (!path) return

    const tool = parsed.searchParams.get('tool') ?? undefined
    const worktree = parsed.searchParams.get('worktree') ?? undefined

    const action = parsed.hostname === 'run' ? 'run' : 'open'

    mainWindow.webContents.send('url:action', { action, path, tool, worktree })
  } catch {
    // Invalid URL — ignore
  }
}

function createWindow(): BrowserWindow {
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

  win.on('ready-to-show', () => {
    win.show()
  })

  win.on('closed', () => {
    disposeGitWatcher()
    wsBridge.disposeAll()
    ptyManager.dispose()
    mainWindow = null
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

  mainWindow = win
  return win
}

app.whenReady().then(async () => {
  // Resolve user's login shell env before anything else
  await resolveLoginEnv()

  electronApp.setAppUserModelId('com.electron')

  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window)
  })

  createWindow()

  claudeSessionManager = new ClaudeSessionManager()
  claudeSessionManager.cleanupOrphans()

  registerIpcHandlers(
    ptyManager,
    wsBridge,
    workspaceStore,
    preferencesStore,
    layoutStore,
    toolRegistry,
    claudeSessionManager,
    () => mainWindow,
  )

  // Handle URL scheme on macOS
  app.on('open-url', (event, url) => {
    event.preventDefault()
    handleNixttyUrl(url)
  })

  // Handle URL scheme on Windows/Linux (second instance)
  app.on('second-instance', (_event, argv) => {
    const url = argv.find((a) => a.startsWith('nixtty://'))
    if (url) handleNixttyUrl(url)
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore()
      mainWindow.focus()
    }
  })

  app.on('activate', function () {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('before-quit', () => {
  claudeSessionManager?.dispose()
  disposeGitWatcher()
  wsBridge.disposeAll()
  ptyManager.dispose()
  database.close()
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
