import { app, BrowserWindow, dialog, ipcMain, Menu, powerMonitor } from 'electron'
import { resolve } from 'path'
import { autoUpdater } from 'electron-updater'
import { electronApp, is, optimizer } from '@electron-toolkit/utils'
import { PtyManager } from './pty/PtyManager'
import { WsBridge } from './pty/WsBridge'
import { Database } from './db/Database'
import { WorkspaceStore } from './db/WorkspaceStore'
import { PreferencesStore } from './db/PreferencesStore'
import { LayoutStore } from './db/LayoutStore'
import { ToolRegistry } from './tools/ToolRegistry'
import { registerIpcHandlers } from './ipc/handlers'
import { ClaudeSessionManager } from './claude/ClaudeSessionManager'
import { resolveLoginEnv } from './shell/loginEnv'
import { WindowManager } from './WindowManager'

if (is.dev) {
  app.setPath('userData', app.getPath('userData') + '-dev')
}

const ptyManager = new PtyManager()
const wsBridge = new WsBridge()
const database = new Database()
const workspaceStore = new WorkspaceStore(database)
const preferencesStore = new PreferencesStore(database)
const layoutStore = new LayoutStore(database)
const toolRegistry = new ToolRegistry(database)
const windowManager = new WindowManager(ptyManager, wsBridge)

let claudeSessionManager: ClaudeSessionManager | null = null

// Register canopy:// URL scheme
if (process.defaultApp) {
  if (process.argv.length >= 2) {
    app.setAsDefaultProtocolClient('canopy', process.execPath, [resolve(process.argv[1])])
  }
} else {
  app.setAsDefaultProtocolClient('canopy')
}

// Ensure single instance (required for URL scheme on Windows/Linux)
const gotTheLock = app.requestSingleInstanceLock()
if (!gotTheLock) {
  app.quit()
}

function handleCanopyUrl(url: string): void {
  try {
    const parsed = new URL(url)
    const path = parsed.searchParams.get('path')
    if (!path) return

    const tool = parsed.searchParams.get('tool') ?? undefined
    const worktree = parsed.searchParams.get('worktree') ?? undefined
    const action = parsed.hostname === 'run' ? 'run' : 'open'

    // Dedupe: focus existing window for this path
    const existing = windowManager.getWindowForPath(path)
    if (existing) {
      existing.webContents.send('url:action', { action, path, tool, worktree })
      if (existing.isMinimized()) existing.restore()
      existing.focus()
      return
    }

    // Open in a new window
    const win = windowManager.createWindow()
    win.once('ready-to-show', () => {
      win.webContents.send('url:action', { action, path, tool, worktree })
    })
  } catch {
    // Invalid URL
  }
}

function buildAppMenu(): void {
  const isMac = process.platform === 'darwin'

  const template: Electron.MenuItemConstructorOptions[] = [
    ...(isMac
      ? [
          {
            label: app.name,
            submenu: [
              { role: 'about' as const },
              { type: 'separator' as const },
              { role: 'hide' as const },
              { role: 'hideOthers' as const },
              { role: 'unhide' as const },
              { type: 'separator' as const },
              { role: 'quit' as const },
            ],
          },
        ]
      : []),
    {
      label: 'File',
      submenu: [
        {
          label: 'New Window',
          accelerator: 'CmdOrCtrl+Shift+N',
          click: () => windowManager.createWindow(),
        },
        { type: 'separator' },
        isMac ? { role: 'close' } : { role: 'quit' },
      ],
    },
    {
      label: 'Edit',
      submenu: [
        { role: 'undo' },
        { role: 'redo' },
        { type: 'separator' },
        { role: 'cut' },
        { role: 'copy' },
        { role: 'paste' },
        { role: 'selectAll' },
      ],
    },
    {
      label: 'Window',
      submenu: [
        { role: 'minimize' },
        { role: 'zoom' },
        ...(isMac
          ? [{ type: 'separator' as const }, { role: 'front' as const }]
          : [{ role: 'close' as const }]),
      ],
    },
    {
      label: 'Help',
      submenu: [
        {
          label: 'Check for Updates…',
          click: () => {
            if (app.isPackaged) {
              autoUpdater.checkForUpdates().catch((err) => {
                console.warn('Manual update check failed:', err)
              })
            }
          },
        },
      ],
    },
  ]

  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

app.whenReady().then(async () => {
  await resolveLoginEnv()

  electronApp.setAppUserModelId('tech.itsol.canopy')

  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window)
  })

  buildAppMenu()

  if (app.isPackaged) {
    autoUpdater.logger = console
    autoUpdater.autoDownload = true
    autoUpdater.allowPrerelease = false

    const broadcast = (channel: string, data: unknown): void => {
      for (const win of BrowserWindow.getAllWindows()) {
        if (!win.isDestroyed()) win.webContents.send(channel, data)
      }
    }

    let updateDownloaded = false

    autoUpdater.on('update-available', (info) => {
      broadcast('update:available', { version: info.version, releaseNotes: info.releaseNotes })
    })

    autoUpdater.on('update-not-available', () => {
      broadcast('update:not-available', {})
    })

    autoUpdater.on('download-progress', (progress) => {
      broadcast('update:progress', {
        percent: progress.percent,
        bytesPerSecond: progress.bytesPerSecond,
        transferred: progress.transferred,
        total: progress.total,
      })
    })

    autoUpdater.on('update-downloaded', (info) => {
      updateDownloaded = true
      broadcast('update:downloaded', { version: info.version, releaseNotes: info.releaseNotes })
    })

    autoUpdater.on('error', (err) => {
      broadcast('update:error', { message: err.message })
    })

    ipcMain.handle('app:checkForUpdates', async () => {
      await autoUpdater.checkForUpdates()
    })

    ipcMain.handle('app:installUpdate', () => {
      if (!updateDownloaded) return
      autoUpdater.quitAndInstall(true, true)
    })

    autoUpdater.checkForUpdates().catch((err) => {
      console.warn('Auto-update check failed:', err)
    })
  }

  claudeSessionManager = new ClaudeSessionManager()
  claudeSessionManager.cleanupOrphans()
  windowManager.setClaudeSessionManager(claudeSessionManager)

  registerIpcHandlers(
    ptyManager,
    wsBridge,
    workspaceStore,
    preferencesStore,
    layoutStore,
    toolRegistry,
    claudeSessionManager,
    windowManager,
  )

  // Force-close stale WebSocket clients on system wake so renderer reconnects
  powerMonitor.on('resume', () => {
    wsBridge.disconnectAllClients()
  })

  // Restore windows from last session
  const reopenPref = preferencesStore.get('reopenLastWorkspace')
  if (reopenPref !== 'false') {
    // Try new multi-project format first
    const configsJson = preferencesStore.get('openWindowConfigs')
    let windowConfigs: Array<{ paths: string[] }> = []

    if (configsJson) {
      try {
        windowConfigs = JSON.parse(configsJson) as Array<{ paths: string[] }>
      } catch {
        // Invalid JSON
      }
    }

    // Fallback to legacy flat format (one path per window)
    if (windowConfigs.length === 0) {
      const pathsJson = preferencesStore.get('openWorkspacePaths')
      if (pathsJson) {
        try {
          const paths = JSON.parse(pathsJson) as string[]
          windowConfigs = paths.map((p) => ({ paths: [p] }))
        } catch {
          // Invalid JSON
        }
      }
    }

    // Fallback to legacy single-path pref
    if (windowConfigs.length === 0) {
      const lastPath = preferencesStore.get('lastWorkspacePath')
      if (lastPath) windowConfigs = [{ paths: [lastPath] }]
    }

    if (windowConfigs.length > 0) {
      for (const config of windowConfigs) {
        const win = windowManager.createWindow()
        win.once('ready-to-show', () => {
          for (const path of config.paths) {
            win.webContents.send('url:action', { action: 'open', path })
          }
        })
      }
    } else {
      windowManager.createWindow()
    }
  } else {
    windowManager.createWindow()
  }

  // Handle URL scheme on macOS
  app.on('open-url', (event, url) => {
    event.preventDefault()
    handleCanopyUrl(url)
  })

  // Handle URL scheme on Windows/Linux (second instance) — also create new window if no URL
  app.on('second-instance', (_event, argv) => {
    const url = argv.find((a) => a.startsWith('canopy://'))
    if (url) {
      handleCanopyUrl(url)
    } else {
      windowManager.createWindow()
    }
  })

  app.on('activate', function () {
    if (BrowserWindow.getAllWindows().length === 0) windowManager.createWindow()
  })
})

app.on('before-quit', (event) => {
  if (!windowManager.isQuitting) {
    const activeInfo = windowManager.hasAnyActiveSession()
    if (activeInfo) {
      event.preventDefault()

      const focusedWin = BrowserWindow.getFocusedWindow() ?? windowManager.getAllWindows()[0]
      if (!focusedWin || focusedWin.isDestroyed()) return

      dialog
        .showMessageBox(focusedWin, {
          type: 'warning',
          buttons: ['Quit', 'Cancel'],
          defaultId: 1,
          cancelId: 1,
          title: 'Active Sessions',
          message: 'There are active sessions running',
          detail: activeInfo,
        })
        .then(({ response }) => {
          if (response === 0) {
            windowManager.isQuitting = true
            app.quit()
          }
        })
      return
    }
  }

  // Normal cleanup — save per-window project configs
  const configs = windowManager.getAllWindowConfigs()
  if (configs.length > 0) {
    preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
  } else {
    preferencesStore.delete('openWindowConfigs')
  }

  claudeSessionManager?.dispose()
  windowManager.disposeAll()
  database.close()
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
