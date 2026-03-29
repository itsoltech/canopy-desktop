import { app, BrowserWindow, dialog, ipcMain, Menu, powerMonitor, shell } from 'electron'
import os from 'os'
import { readFileSync, realpathSync } from 'fs'
import { join, resolve, sep } from 'path'
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
import { BrowserManager } from './browser/BrowserManager'
import { NotchOverlayManager } from './notch/NotchOverlayManager'
import { isSafeExternalUrl } from './security/validateUrl'

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
const browserManager = new BrowserManager()
let manualCheckInProgress = false
let updateInstalling = false

let claudeSessionManager: ClaudeSessionManager | null = null
let notchOverlay: NotchOverlayManager | null = null

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

async function handleCanopyUrl(url: string): Promise<void> {
  try {
    const parsed = new URL(url)
    const path = parsed.searchParams.get('path')
    if (!path) return

    // Validate path is under user's home directory (resolve symlinks to prevent bypass)
    let resolved: string
    let home: string
    try {
      resolved = realpathSync(resolve(path))
      home = realpathSync(os.homedir())
    } catch {
      return // Path doesn't exist
    }
    if (!resolved.startsWith(home + sep)) return

    const tool = parsed.searchParams.get('tool') ?? undefined
    const worktree = parsed.searchParams.get('worktree') ?? undefined
    const action = parsed.hostname === 'run' ? 'run' : 'open'

    // Dedupe: focus existing window for this path (no confirmation needed)
    const existing = windowManager.getWindowForPath(resolved)
    if (existing) {
      existing.webContents.send('url:action', { action, path: resolved, tool, worktree })
      if (existing.isMinimized()) existing.restore()
      existing.focus()
      return
    }

    // Confirm before opening a new workspace from external deep link
    const { response } = await dialog.showMessageBox({
      type: 'question',
      buttons: ['Open', 'Cancel'],
      defaultId: 1,
      cancelId: 1,
      message: 'Open workspace?',
      detail: `An external application wants to open:\n${resolved}`,
    })
    if (response !== 0) return

    const win = windowManager.createWindow()
    win.once('ready-to-show', () => {
      win.webContents.send('url:action', { action, path: resolved, tool, worktree })
    })
  } catch {
    // Invalid URL
  }
}

function buildAppMenu(): void {
  const isMac = process.platform === 'darwin'

  const showAboutClick = (): void => {
    const win = BrowserWindow.getFocusedWindow()
    if (win && !win.isDestroyed()) {
      win.webContents.send('menu:showAbout')
    }
  }

  const showPreferencesClick = (): void => {
    const win = BrowserWindow.getFocusedWindow()
    if (win && !win.isDestroyed()) {
      win.webContents.send('menu:showPreferences')
    }
  }

  const checkForUpdatesClick = (): void => {
    if (app.isPackaged) {
      manualCheckInProgress = true
      autoUpdater.checkForUpdates().catch((err) => {
        manualCheckInProgress = false
        console.warn('Manual update check failed:', err)
      })
    }
  }

  const template: Electron.MenuItemConstructorOptions[] = [
    ...(isMac
      ? [
          {
            label: app.name,
            submenu: [
              { label: 'About Canopy', click: showAboutClick },
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
        ...(!isMac
          ? [
              {
                label: 'Settings…',
                accelerator: 'CmdOrCtrl+,',
                click: showPreferencesClick,
              },
              { type: 'separator' as const },
              { label: 'Check for Updates…', click: checkForUpdatesClick },
              { label: 'About Canopy', click: showAboutClick },
              { type: 'separator' as const },
            ]
          : []),
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
        { label: 'Check for Updates…', click: checkForUpdatesClick },
        { type: 'separator' as const },
        {
          label: 'Privacy Policy',
          click: () => {
            shell.openExternal('https://canopy.itsol.tech/privacy-policy')
          },
        },
        {
          label: 'Third-Party Licenses',
          click: () => {
            const noticesPath = app.isPackaged
              ? resolve(process.resourcesPath, 'THIRD-PARTY-NOTICES')
              : resolve(app.getAppPath(), 'THIRD-PARTY-NOTICES')
            shell.openPath(noticesPath)
          },
        },
        ...(!isMac
          ? [{ type: 'separator' as const }, { label: 'About Canopy', click: showAboutClick }]
          : []),
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
      manualCheckInProgress = false
      broadcast('update:available', { version: info.version, releaseNotes: info.releaseNotes })
    })

    autoUpdater.on('update-not-available', () => {
      if (manualCheckInProgress) {
        broadcast('update:not-available', {})
        manualCheckInProgress = false
      }
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

    ipcMain.handle('app:checkForUpdates', () => {
      manualCheckInProgress = true
      autoUpdater.checkForUpdates().catch((err) => {
        manualCheckInProgress = false
        console.warn('Manual update check failed:', err)
      })
    })

    ipcMain.handle('app:installUpdate', () => {
      if (!updateDownloaded) return
      const configs = windowManager.getAllWindowConfigs()
      if (configs.length > 0) {
        preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
      } else {
        preferencesStore.delete('openWindowConfigs')
      }
      updateInstalling = true
      windowManager.isQuitting = true
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
    browserManager,
  )

  ipcMain.handle('app:openExternal', (_event, { url }: { url: string }) => {
    if (!isSafeExternalUrl(url)) return
    return shell.openExternal(url)
  })

  ipcMain.handle('app:quit', () => {
    app.quit()
  })

  ipcMain.handle('app:openThirdPartyNotices', () => {
    const noticesPath = app.isPackaged
      ? resolve(process.resourcesPath, 'THIRD-PARTY-NOTICES')
      : resolve(app.getAppPath(), 'THIRD-PARTY-NOTICES')
    return shell.openPath(noticesPath)
  })

  ipcMain.handle('app:getAboutInfo', () => ({
    version: app.getVersion(),
    homepage: 'https://canopy.itsol.tech',
    license: readFileSync(join(app.getAppPath(), 'LICENSE.md'), 'utf-8'),
  }))

  // Force-close stale WebSocket clients on system wake so renderer reconnects
  powerMonitor.on('resume', () => {
    wsBridge.disconnectAllClients()
  })

  // Restore windows from last session
  const reopenPref = preferencesStore.get('reopenLastWorkspace')
  if (reopenPref !== 'false') {
    // Try new multi-project format first
    const configsJson = preferencesStore.get('openWindowConfigs')
    let windowConfigs: Array<{ paths: string[]; activeWorktreePath?: string }> = []

    if (configsJson) {
      try {
        windowConfigs = JSON.parse(configsJson) as Array<{
          paths: string[]
          activeWorktreePath?: string
        }>
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
          if (config.activeWorktreePath) {
            win.webContents.send('workspace:restoreActive', config.activeWorktreePath)
          }
        })
      }
    } else {
      windowManager.createWindow()
    }
  } else {
    windowManager.createWindow()
  }

  // Initialize notch overlay after main window so the panel doesn't suppress the dock icon
  notchOverlay = new NotchOverlayManager(claudeSessionManager, windowManager)
  if (preferencesStore.get('notch.enabled') === 'true') {
    notchOverlay.initialize()
  }

  ipcMain.on('notch:setEnabled', (event, { enabled }: { enabled: boolean }) => {
    if (!notchOverlay) return
    if (!windowManager.getWindowById(BrowserWindow.fromWebContents(event.sender)?.id ?? -1)) return
    if (enabled) {
      notchOverlay.initialize()
    } else {
      notchOverlay.dispose()
    }
  })

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

  // Save per-window project configs (skip if update already saved them)
  if (!updateInstalling) {
    const configs = windowManager.getAllWindowConfigs()
    if (configs.length > 0) {
      preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
    } else {
      preferencesStore.delete('openWindowConfigs')
    }
  }

  notchOverlay?.dispose()
  claudeSessionManager?.dispose()
  windowManager.disposeAll()
  database.close()
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
