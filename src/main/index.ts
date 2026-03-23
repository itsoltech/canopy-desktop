import { app, shell, BrowserWindow } from 'electron'
import { join } from 'path'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'
import icon from '../../resources/icon.png?asset'
import { PtyManager } from './pty/PtyManager'
import { WsBridge } from './pty/WsBridge'
import { Database } from './db/Database'
import { WorkspaceStore } from './db/WorkspaceStore'
import { PreferencesStore } from './db/PreferencesStore'
import { ToolRegistry } from './tools/ToolRegistry'
import { registerIpcHandlers, disposeGitWatcher } from './ipc/handlers'
import { ClaudeSessionManager } from './claude/ClaudeSessionManager'
import { resolveLoginEnv } from './shell/loginEnv'

const ptyManager = new PtyManager()
const wsBridge = new WsBridge()
const database = new Database()
const workspaceStore = new WorkspaceStore(database)
const preferencesStore = new PreferencesStore(database)
const toolRegistry = new ToolRegistry(database)

let mainWindow: BrowserWindow | null = null
let claudeSessionManager: ClaudeSessionManager | null = null

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

  claudeSessionManager = new ClaudeSessionManager(mainWindow!)
  claudeSessionManager.cleanupOrphans()

  registerIpcHandlers(
    ptyManager,
    wsBridge,
    workspaceStore,
    preferencesStore,
    toolRegistry,
    claudeSessionManager,
    () => mainWindow,
  )

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
