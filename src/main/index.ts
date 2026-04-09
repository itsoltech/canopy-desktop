import { app, BrowserWindow, dialog, ipcMain, Menu, powerMonitor, shell } from 'electron'
import os from 'os'
import { existsSync, readFileSync, realpathSync } from 'fs'
import { join, resolve, sep } from 'path'
import { autoUpdater } from 'electron-updater'
import { match } from 'ts-pattern'
import { electronApp, is, optimizer } from '@electron-toolkit/utils'
import { PtyManager } from './pty/PtyManager'
import { WsBridge } from './pty/WsBridge'
import { Database } from './db/Database'
import { WorkspaceStore } from './db/WorkspaceStore'
import { PreferencesStore } from './db/PreferencesStore'
import { LayoutStore } from './db/LayoutStore'
import { OnboardingStore } from './db/OnboardingStore'
import { ToolRegistry } from './tools/ToolRegistry'
import { registerIpcHandlers } from './ipc/handlers'
import { AgentSessionManager } from './agents/AgentSessionManager'
import { resolveLoginEnv } from './shell/loginEnv'
import { WindowManager } from './WindowManager'
import { BrowserManager } from './browser/BrowserManager'
import { CredentialStore } from './db/CredentialStore'
import { NotchOverlayManager } from './notch/NotchOverlayManager'
import { TmuxManager } from './pty/TmuxManager'
import { TaskTrackerManager } from './taskTracker/TaskTrackerManager'
import { KeychainTokenStore } from './taskTracker/KeychainTokenStore'
import { RepoConfigManager } from './taskTracker/RepoConfigManager'
import { GlobalConfigManager } from './taskTracker/GlobalConfigManager'
import { GitHubService } from './github/GitHubService'
import semver from 'semver'
import { isSafeExternalUrl } from './security/validateUrl'
import { fetchChangelogRange, resolveUpdateChannel } from './changelog/fetchChangelog'
import { validateBounds, cascadeBounds } from './windowBounds'
import { TelemetryManager } from './telemetry/TelemetryManager'
import { RemoteSessionService } from './remote/RemoteSessionService'
import type { WindowConfig } from './windowBounds'
import { performance } from 'perf_hooks'

const PERF = process.env.CANOPY_PERF === '1'
if (PERF) performance.mark('app:init')

// IPC traffic log for perf testing (only allocated when CANOPY_PERF=1)
interface IpcLogEntry {
  channel: string
  size: number
  ts: number
  dir: 'in' | 'out'
}
const ipcLog: IpcLogEntry[] | null = PERF ? [] : null
const MAX_IPC_LOG_ENTRIES = 50_000

if (PERF) {
  // Monkey-patches ipcMain.handle/on to log IPC traffic. Must run before
  // registerIpcHandlers() (called in app.whenReady) so all handlers get wrapped.
  const origHandle = ipcMain.handle.bind(ipcMain)
  ipcMain.handle = (channel: string, listener: Parameters<typeof ipcMain.handle>[1]) => {
    return origHandle(channel, (event, ...args) => {
      if (!channel.startsWith('perf:') && ipcLog!.length < MAX_IPC_LOG_ENTRIES) {
        ipcLog!.push({
          channel,
          size: typeof args[0] === 'string' ? args[0].length : 0,
          ts: Date.now(),
          dir: 'in',
        })
      }
      return listener(event, ...args)
    })
  }

  const origOn = ipcMain.on.bind(ipcMain)
  ipcMain.on = (channel: string, listener: Parameters<typeof ipcMain.on>[1]) => {
    return origOn(channel, (event, ...args) => {
      if (!channel.startsWith('perf:') && ipcLog!.length < MAX_IPC_LOG_ENTRIES) {
        ipcLog!.push({
          channel,
          size: typeof args[0] === 'string' ? args[0].length : 0,
          ts: Date.now(),
          dir: 'in',
        })
      }
      return (listener as (...a: unknown[]) => void)(event, ...args)
    })
  }
}

if (is.dev) {
  app.setPath('userData', app.getPath('userData') + '-dev')
}

if (process.env.CANOPY_TEST_USER_DATA && !app.isPackaged) {
  app.setPath('userData', process.env.CANOPY_TEST_USER_DATA)
}

const ptyManager = new PtyManager()
const wsBridge = new WsBridge()
const database = new Database()
const workspaceStore = new WorkspaceStore(database)
const preferencesStore = new PreferencesStore(database)
const layoutStore = new LayoutStore(database)
const onboardingStore = new OnboardingStore(database)
const toolRegistry = new ToolRegistry(database)
const telemetryManager = new TelemetryManager(preferencesStore)
const windowManager = new WindowManager(ptyManager, wsBridge)
const browserManager = new BrowserManager()
const credentialStore = new CredentialStore(database)
const tmuxManager = new TmuxManager(app.getPath('userData'))
const remoteSessionService = new RemoteSessionService(preferencesStore)
windowManager.setTmuxManager(tmuxManager)
windowManager.setOnWindowDispose((paths) => {
  for (const path of paths) {
    const ws = workspaceStore.getByPath(path)
    if (ws) layoutStore.deleteAll(ws.id)
  }
})
let manualCheckInProgress = false
let updateInstalling = false
let updateCheckInFlight = false
let updateCheckIntervalTimer: ReturnType<typeof setInterval> | null = null

type UpdateCheckFrequency = 'never' | 'hourly' | 'daily' | 'weekly'

const HOUR_MS = 60 * 60 * 1000
const DAY_MS = 24 * HOUR_MS
const WEEK_MS = 7 * DAY_MS

const normalizeUpdateCheckFrequency = (value: string | null): UpdateCheckFrequency =>
  match(value)
    .with('never', 'hourly', 'daily', 'weekly', (v) => v)
    .otherwise(() => 'daily' as const)

const getUpdateCheckIntervalMs = (frequency: UpdateCheckFrequency): number | null =>
  match(frequency)
    .with('hourly', () => HOUR_MS)
    .with('daily', () => DAY_MS)
    .with('weekly', () => WEEK_MS)
    .with('never', () => null)
    .exhaustive()

const checkWithChannelResolution = async (): Promise<void> => {
  if (updateCheckInFlight) return
  updateCheckInFlight = true
  try {
    const ch = preferencesStore.get('update.channel') ?? 'stable'
    if (ch === 'next') {
      const effective = await resolveUpdateChannel(app.getVersion()).unwrapOr('next' as const)
      autoUpdater.channel = effective
      autoUpdater.allowPrerelease = true
    } else {
      autoUpdater.channel = 'latest'
      autoUpdater.allowPrerelease = false
    }
    await autoUpdater.checkForUpdates()
  } finally {
    updateCheckInFlight = false
  }
}

const scheduleRecurringUpdateCheck = (): void => {
  if (updateCheckIntervalTimer) {
    clearInterval(updateCheckIntervalTimer)
    updateCheckIntervalTimer = null
  }
  const frequency = normalizeUpdateCheckFrequency(preferencesStore.get('update.checkFrequency'))
  const intervalMs = getUpdateCheckIntervalMs(frequency)
  if (intervalMs === null) return
  updateCheckIntervalTimer = setInterval(() => {
    checkWithChannelResolution().catch((err) => {
      console.warn('Scheduled update check failed:', err)
    })
  }, intervalMs)
}

let agentSessionManager: AgentSessionManager | null = null
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
      checkWithChannelResolution().catch((err) => {
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
              {
                label: 'Preferences…',
                accelerator: 'CmdOrCtrl+,',
                click: showPreferencesClick,
              },
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
          click: () =>
            windowManager.createWindow({
              bounds: cascadeBounds(windowManager.getLastFocusedBounds()),
            }),
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
  if (PERF) performance.mark('app:ready')
  await resolveLoginEnv()
  if (PERF) performance.mark('app:loginEnvResolved')

  electronApp.setAppUserModelId('tech.itsol.canopy')

  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window)
    window.on('focus', () => telemetryManager.onWindowFocus())
  })

  buildAppMenu()

  // Track version changes for post-update changelog / onboarding
  const currentVersion = app.getVersion()
  const lastSeenVersion = preferencesStore.get('app.lastSeenVersion')
  const isFirstLaunch = lastSeenVersion === null
  const versionChanged = !isFirstLaunch && lastSeenVersion !== currentVersion
  preferencesStore.set('app.lastSeenVersion', currentVersion)

  if (app.isPackaged) {
    telemetryManager.init()

    const updateChannel = preferencesStore.get('update.channel') ?? 'stable'
    const autoUpdate = preferencesStore.get('update.autoUpdate') !== 'false'

    autoUpdater.logger = console
    autoUpdater.autoDownload = autoUpdate
    autoUpdater.allowPrerelease = updateChannel === 'next'

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
      autoUpdater.autoInstallOnAppQuit = true
      broadcast('update:downloaded', { version: info.version, releaseNotes: info.releaseNotes })
    })

    autoUpdater.on('error', (err) => {
      broadcast('update:error', { message: err.message })
    })

    ipcMain.handle('app:setUpdateChannel', (_e, channel: string) => {
      if (channel !== 'stable' && channel !== 'next') return
      preferencesStore.set('update.channel', channel)
      checkWithChannelResolution().catch((err) => {
        console.warn('Update check after channel switch failed:', err)
      })
    })

    ipcMain.handle('app:setAutoUpdate', (_e, enabled: boolean) => {
      autoUpdater.autoDownload = enabled
      preferencesStore.set('update.autoUpdate', enabled ? 'true' : 'false')
    })

    ipcMain.handle('app:setUpdateCheckFrequency', (_e, frequency: string) => {
      const normalized = normalizeUpdateCheckFrequency(frequency)
      preferencesStore.set('update.checkFrequency', normalized)
      scheduleRecurringUpdateCheck()
    })

    ipcMain.handle('app:checkForUpdates', () => {
      manualCheckInProgress = true
      checkWithChannelResolution().catch((err) => {
        manualCheckInProgress = false
        console.warn('Manual update check failed:', err)
      })
    })

    ipcMain.handle('app:installUpdate', async () => {
      if (!updateDownloaded || updateInstalling) return

      console.log('[updater] installUpdate requested')

      const configs = windowManager.getAllWindowConfigs()
      if (configs.length > 0) {
        preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
      } else {
        preferencesStore.delete('openWindowConfigs')
      }

      updateInstalling = true
      windowManager.isQuitting = true

      // Broadcast installing state and give renderer time to render it
      for (const win of BrowserWindow.getAllWindows()) {
        if (!win.isDestroyed()) win.webContents.send('update:installing', {})
      }
      await new Promise((resolve) => setTimeout(resolve, 500))

      // Destroy all windows to prevent lifecycle conflicts
      const closePromises = BrowserWindow.getAllWindows().map(
        (win) =>
          new Promise<void>((resolve) => {
            if (win.isDestroyed()) return resolve()
            win.once('closed', () => resolve())
            win.destroy()
          }),
      )
      await Promise.all(closePromises)
      console.log('[updater] all windows destroyed')

      app.releaseSingleInstanceLock()
      setImmediate(() => {
        autoUpdater.quitAndInstall(true, true)
      })

      // Safety net: force exit if quit hangs
      setTimeout(() => {
        console.error('[updater] quit did not complete within 10s — forcing exit')
        app.exit(0)
      }, 10_000)
    })

    const checkFrequency = normalizeUpdateCheckFrequency(
      preferencesStore.get('update.checkFrequency'),
    )
    if (checkFrequency !== 'never') {
      checkWithChannelResolution().catch((err) => {
        console.warn('Auto-update check failed:', err)
      })
      scheduleRecurringUpdateCheck()
    }
  }

  // SECURITY: Validate and harden all <webview> tags before they attach.
  // Even if an attacker modifies webview attributes in the DOM, this handler
  // forces safe webPreferences and blocks non-http(s) sources.
  app.on('web-contents-created', (_event, contents) => {
    contents.on('will-attach-webview', (event, webPreferences, params) => {
      // Strip preload scripts — browser webviews must not have preload
      delete webPreferences.preload

      // Force secure defaults
      webPreferences.nodeIntegration = false
      webPreferences.contextIsolation = true
      webPreferences.sandbox = true

      // Only allow http(s) or about:blank as source
      const src = params.src
      if (src && src !== '' && src !== 'about:blank') {
        try {
          const url = new URL(src)
          if (url.protocol !== 'http:' && url.protocol !== 'https:') {
            event.preventDefault()
            return
          }
        } catch {
          event.preventDefault()
          return
        }
      }
    })
  })

  // Initialize browser partition (shared session for all browser webviews,
  // isolated from the main app session to protect API keys)
  browserManager.ensurePartition()

  agentSessionManager = new AgentSessionManager()
  agentSessionManager.cleanupOrphans()
  windowManager.setAgentSessionManager(agentSessionManager)
  windowManager.setBrowserManager(browserManager)

  const keychainTokenStore = new KeychainTokenStore(preferencesStore)
  const repoConfigManager = new RepoConfigManager()
  const globalConfigManager = new GlobalConfigManager(preferencesStore, keychainTokenStore)
  const { RunConfigManager } = await import('./runConfig/RunConfigManager')
  const runConfigManager = new RunConfigManager()
  const taskTrackerManager = new TaskTrackerManager(preferencesStore, keychainTokenStore)
  const gitHubService = new GitHubService(preferencesStore, taskTrackerManager)

  if (PERF) performance.mark('app:managersReady')

  registerIpcHandlers(
    ptyManager,
    wsBridge,
    workspaceStore,
    preferencesStore,
    layoutStore,
    toolRegistry,
    agentSessionManager,
    windowManager,
    browserManager,
    credentialStore,
    onboardingStore,
    tmuxManager,
    taskTrackerManager,
    repoConfigManager,
    globalConfigManager,
    keychainTokenStore,
    gitHubService,
    remoteSessionService,
    runConfigManager,
  )

  if (PERF) performance.mark('app:ipcHandlersRegistered')

  if (PERF) {
    // Log outgoing broadcasts for all current and future windows
    app.on('browser-window-created', (_, win) => {
      const wc = win.webContents
      const origSend = wc.send.bind(wc)
      wc.send = (channel: string, ...args: unknown[]) => {
        if (!channel.startsWith('perf:') && ipcLog!.length < MAX_IPC_LOG_ENTRIES) {
          ipcLog!.push({
            channel,
            size: typeof args[0] === 'string' ? args[0].length : 0,
            ts: Date.now(),
            dir: 'out',
          })
        }
        return origSend(channel, ...args)
      }
    })

    // Also patch existing windows (the one already created during this tick)
    for (const win of BrowserWindow.getAllWindows()) {
      const wc = win.webContents
      const origSend = wc.send.bind(wc)
      wc.send = (channel: string, ...args: unknown[]) => {
        if (!channel.startsWith('perf:') && ipcLog!.length < MAX_IPC_LOG_ENTRIES) {
          ipcLog!.push({
            channel,
            size: typeof args[0] === 'string' ? args[0].length : 0,
            ts: Date.now(),
            dir: 'out',
          })
        }
        return origSend(channel, ...args)
      }
    }

    ipcMain.handle('perf:diagnostics', () => ({
      ptySessionCount: ptyManager.sessionCount,
      wsBridgeCount: wsBridge.bridgeCount,
      agentSessionCount: agentSessionManager?.sessionCount ?? 0,
      gitWatcherCount: windowManager.gitWatcherCount,
      windowCount: windowManager.size,
      heapUsed: process.memoryUsage().heapUsed,
      rss: process.memoryUsage().rss,
      uptime: process.uptime(),
      marks: performance
        .getEntriesByType('mark')
        .map((m) => ({ name: m.name, startTime: m.startTime })),
    }))

    ipcMain.handle('perf:ipcLog', () => {
      const snapshot = [...ipcLog!]
      ipcLog!.length = 0
      return snapshot
    })
  }

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

  ipcMain.handle(
    'app:getChangelogSinceVersion',
    async (_e, { fromVersion }: { fromVersion: string }) => {
      if (typeof fromVersion !== 'string' || !semver.valid(fromVersion)) return null
      const channel = (preferencesStore.get('update.channel') ?? 'stable') as 'stable' | 'next'
      return fetchChangelogRange(fromVersion, app.getVersion(), channel).unwrapOr(null)
    },
  )

  // Force-close stale WebSocket clients on system wake so renderer reconnects
  powerMonitor.on('resume', () => {
    wsBridge.disconnectAllClients()
  })

  // Restore windows from last session
  const reopenPref = preferencesStore.get('reopenLastWorkspace')
  if (reopenPref !== 'false') {
    const configsJson = preferencesStore.get('openWindowConfigs')
    let windowConfigs: WindowConfig[] = []

    if (configsJson) {
      try {
        windowConfigs = JSON.parse(configsJson) as WindowConfig[]
      } catch {
        // Invalid JSON
      }
    }

    // Restore workspaces that have layouts but aren't in any window config
    const configPaths = new Set(windowConfigs.flatMap((c) => c.paths))
    const layoutWsIds = layoutStore.getDistinctWorkspaceIds()
    for (const wsId of layoutWsIds) {
      const ws = workspaceStore.get(wsId)
      if (ws && !configPaths.has(ws.path)) {
        windowConfigs.push({ paths: [ws.path] })
      }
    }

    // Drop any paths whose directory no longer exists on disk. Without this,
    // restore would fail every launch with a toast, leaving an unreachable
    // stale row in the workspaces table (see itsoltech/canopy-desktop#128).
    const allRemovedPaths: string[] = []
    for (const config of windowConfigs) {
      const keptPaths: string[] = []
      for (const p of config.paths) {
        if (existsSync(p)) {
          keptPaths.push(p)
        } else {
          allRemovedPaths.push(p)
        }
      }
      config.paths = keptPaths
      if (config.activeWorktreePath && !existsSync(config.activeWorktreePath)) {
        config.activeWorktreePath = undefined
      }
    }
    if (allRemovedPaths.length > 0) {
      console.info('[restore] dropping stale paths (folder missing):', allRemovedPaths)
      for (const p of allRemovedPaths) {
        const ws = workspaceStore.getByPath(p)
        if (ws) {
          layoutStore.deleteAll(ws.id)
          workspaceStore.remove(ws.id)
        } else {
          console.warn(
            `[restore] no workspace row found for stale path "${p}" — skipping DB cleanup`,
          )
        }
      }
      // Drop configs that are now empty after stale-path removal — otherwise
      // they'd spawn blank ghost windows and get persisted back every restart.
      windowConfigs = windowConfigs.filter((c) => c.paths.length > 0)
      if (windowConfigs.length > 0) {
        preferencesStore.set('openWindowConfigs', JSON.stringify(windowConfigs))
      } else {
        preferencesStore.delete('openWindowConfigs')
      }
    }

    let postLaunchSent = false
    const sendPostLaunch = (win: BrowserWindow): void => {
      if (postLaunchSent) return
      postLaunchSent = true
      if (PERF) performance.mark('app:firstWindowReady')

      if (isFirstLaunch) {
        win.webContents.send('app:showOnboarding', { mode: 'first-launch' })
      } else if (versionChanged && lastSeenVersion) {
        win.webContents.send('app:showOnboarding', {
          mode: 'upgrade',
          fromVersion: lastSeenVersion,
        })
      }
    }

    if (windowConfigs.length > 0) {
      let removedPathsReported = false
      for (const config of windowConfigs) {
        const bounds = config.bounds ? validateBounds(config.bounds) : undefined
        const win = windowManager.createWindow({
          bounds,
          windowState: config.windowState,
        })
        // Only surface the stale-cleanup toast in one window to avoid duplicates
        const removedPaths =
          !removedPathsReported && allRemovedPaths.length > 0 ? allRemovedPaths : undefined
        if (removedPaths) removedPathsReported = true
        win.once('ready-to-show', () => {
          win.webContents.send('workspace:restoreWindow', {
            paths: config.paths,
            activeWorktreePath: config.activeWorktreePath,
            removedPaths,
          })
          sendPostLaunch(win)
        })
      }
    } else {
      const win = windowManager.createWindow()
      win.once('ready-to-show', () => {
        // If every saved config ended up empty (all projects deleted),
        // we still need to surface the stale-cleanup toast here — the
        // windowed restore branch above would have handled it otherwise.
        if (allRemovedPaths.length > 0) {
          win.webContents.send('workspace:restoreWindow', {
            paths: [],
            removedPaths: allRemovedPaths,
          })
        }
        sendPostLaunch(win)
      })
    }
  } else {
    const win = windowManager.createWindow()
    win.once('ready-to-show', () => {
      if (isFirstLaunch) {
        win.webContents.send('app:showOnboarding', { mode: 'first-launch' })
      } else if (versionChanged && lastSeenVersion) {
        win.webContents.send('app:showOnboarding', {
          mode: 'upgrade',
          fromVersion: lastSeenVersion,
        })
      }
    })
  }

  // Initialize notch overlay after main window so the panel doesn't suppress the dock icon
  notchOverlay = new NotchOverlayManager(agentSessionManager, windowManager)
  if (preferencesStore.get('notch.enabled') === 'true') {
    notchOverlay.initialize()
  }

  // Destroy notch overlay when all managed windows close so window-all-closed fires on Windows
  windowManager.onAllWindowsClosed(() => {
    notchOverlay?.dispose()
  })

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
      windowManager.createWindow({ bounds: cascadeBounds(windowManager.getLastFocusedBounds()) })
    }
  })

  app.on('activate', function () {
    if (BrowserWindow.getAllWindows().length === 0)
      windowManager.createWindow({ bounds: cascadeBounds(windowManager.getLastFocusedBounds()) })
  })
})

app.on('before-quit', (event) => {
  // During update install, skip cleanup that could interfere with Squirrel.
  // Window configs already saved; windows already destroyed.
  if (updateInstalling) {
    if (updateCheckIntervalTimer) {
      clearInterval(updateCheckIntervalTimer)
      updateCheckIntervalTimer = null
    }
    notchOverlay?.dispose()
    agentSessionManager?.dispose()
    remoteSessionService.dispose()
    database.close()
    return
  }

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

  // Always save window configs before any async quit path.
  // The tmux-ask branch below returns early and re-enters via app.quit(),
  // so this is the only reliable place to persist.
  const configs = windowManager.getAllWindowConfigs()
  if (configs.length > 0) {
    preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
  } else {
    preferencesStore.delete('openWindowConfigs')
  }

  // Handle tmux close policy synchronously before any async work
  const tmuxClosePolicy = preferencesStore.get('tmux.closePolicy') ?? 'detach'
  if (tmuxClosePolicy === 'ask' && !windowManager.isQuitting) {
    // preventDefault must be called synchronously — cannot await before this
    event.preventDefault()
    tmuxManager
      .listSessions()
      .catch(() => [])
      .then(async (tmuxSessions) => {
        if (tmuxSessions.length > 0) {
          const focusedWin = BrowserWindow.getFocusedWindow() ?? windowManager.getAllWindows()[0]
          if (focusedWin && !focusedWin.isDestroyed()) {
            const { response } = await dialog.showMessageBox(focusedWin, {
              type: 'question',
              buttons: ['Keep Running', 'Kill Sessions', 'Cancel'],
              defaultId: 0,
              cancelId: 2,
              title: 'Tmux Sessions',
              message: `${tmuxSessions.length} tmux session(s) are running`,
              detail: 'Keep them running in the background or kill them?',
            })
            if (response === 2) return // Cancel — app stays open
            if (response === 1) {
              await tmuxManager.killServer().catch(() => {})
            }
          }
        }
        windowManager.isQuitting = true
        app.quit()
      })
    return
  }

  if (tmuxClosePolicy === 'kill') {
    tmuxManager.killServer().catch(() => {})
  }

  // From this point onward we are intentionally shutting down the app.
  // Window teardown should detach tmux-backed PTYs unless the policy above
  // explicitly killed the tmux server.
  windowManager.isQuitting = true

  if (updateCheckIntervalTimer) {
    clearInterval(updateCheckIntervalTimer)
    updateCheckIntervalTimer = null
  }
  notchOverlay?.dispose()
  agentSessionManager?.dispose()
  remoteSessionService.dispose()
  windowManager.disposeAll()
  database.close()
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
