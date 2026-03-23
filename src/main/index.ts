import { app, BrowserWindow, Menu } from 'electron'
import { resolve } from 'path'
import { electronApp, optimizer } from '@electron-toolkit/utils'
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
  ]

  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

app.whenReady().then(async () => {
  await resolveLoginEnv()

  electronApp.setAppUserModelId('com.electron')

  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window)
  })

  buildAppMenu()

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
    windowManager,
  )

  // Restore windows from last session
  const reopenPref = preferencesStore.get('reopenLastWorkspace')
  if (reopenPref !== 'false') {
    const pathsJson = preferencesStore.get('openWorkspacePaths')
    let paths: string[] = []
    if (pathsJson) {
      try {
        paths = JSON.parse(pathsJson) as string[]
      } catch {
        // Invalid JSON
      }
    }

    // Fallback to legacy single-path pref
    if (paths.length === 0) {
      const lastPath = preferencesStore.get('lastWorkspacePath')
      if (lastPath) paths = [lastPath]
    }

    if (paths.length > 0) {
      for (const path of paths) {
        const win = windowManager.createWindow()
        win.once('ready-to-show', () => {
          win.webContents.send('url:action', { action: 'open', path })
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

app.on('before-quit', () => {
  // Save open workspace paths for restore
  const paths = windowManager.getAllWorkspacePaths()
  if (paths.length > 0) {
    preferencesStore.set('openWorkspacePaths', JSON.stringify(paths))
  } else {
    preferencesStore.delete('openWorkspacePaths')
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
