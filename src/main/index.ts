import { app, BrowserWindow, dialog, ipcMain, Menu, powerMonitor, shell } from 'electron'
import os from 'os'
import { existsSync, realpathSync } from 'fs'
import { readFile } from 'fs/promises'
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
import { initSkills } from './skills'
import { ProfileStore } from './profiles/ProfileStore'
import { registerIpcHandlers, type IpcCommandBridge } from './ipc/handlers'
import { AgentSessionManager } from './agents/AgentSessionManager'
import { resolveLoginEnv } from './shell/loginEnv'
import { WindowManager } from './WindowManager'
import { BrowserManager } from './browser/BrowserManager'
import { CredentialStore } from './db/CredentialStore'
import { SettingsExportService } from './settings/SettingsExport'
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
import { PerfHudService } from './perf/PerfHudService'
import { CrashReporter } from './crash/CrashReporter'
import type { WindowConfig } from './windowBounds'
import { performance } from 'perf_hooks'
import { GitRepository } from './git/GitRepository'

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
const {
  registry: skillRegistry,
  installer: skillInstaller,
  store: skillStore,
} = initSkills(database)
const profileStore = new ProfileStore(database, preferencesStore)
const telemetryManager = new TelemetryManager(preferencesStore)
const windowManager = new WindowManager(ptyManager, wsBridge)
const browserManager = new BrowserManager()
const credentialStore = new CredentialStore(database)
const settingsExportService = new SettingsExportService(
  database,
  preferencesStore,
  profileStore,
  credentialStore,
  toolRegistry,
)
const tmuxManager = new TmuxManager(app.getPath('userData'))
const remoteSessionService = new RemoteSessionService(preferencesStore)
const perfHudService = new PerfHudService()
windowManager.setTmuxManager(tmuxManager)
let lastClosedWindowConfigs: WindowConfig[] | null = null
windowManager.setOnWindowCloseSnapshot(({ configs, isLastWindow }) => {
  if (!isLastWindow || !windowManager.shouldQuitOnLastWindowClose()) return
  lastClosedWindowConfigs = configs
  persistOpenWindowConfigs(configs)
})
windowManager.setOnWindowConfigChanged((configs) => {
  persistOpenWindowConfigs(configs)
})
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
let forceExitTimer: ReturnType<typeof setTimeout> | null = null

type UpdateCheckFrequency = 'never' | 'hourly' | 'daily' | 'weekly'

async function readLicenseText(): Promise<string> {
  const candidates = app.isPackaged
    ? [join(app.getAppPath(), 'LICENSE.md'), resolve(process.resourcesPath, 'LICENSE.md')]
    : [
        join(app.getAppPath(), 'LICENSE.md'),
        resolve(app.getAppPath(), '..', '..', 'LICENSE.md'),
        resolve('LICENSE.md'),
      ]

  for (const candidate of candidates) {
    try {
      return await readFile(candidate, 'utf-8')
    } catch {
      // Try the next dev/packaged location.
    }
  }

  return ''
}

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
      const effective = await resolveUpdateChannel(app.getVersion()).unwrapOr('latest' as const)
      autoUpdater.channel = effective
      autoUpdater.allowPrerelease = effective === 'next'
    } else {
      autoUpdater.channel = 'latest'
      autoUpdater.allowPrerelease = false
    }
    // electron-updater's channel setter implicitly sets allowDowngrade = true, which
    // would downgrade us to a pre-release of the currently installed stable. Reset it.
    autoUpdater.allowDowngrade = false
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
let crashReporter: CrashReporter | null = null
let ipcCommandBridge: IpcCommandBridge | null = null

function canCreateApplicationWindow(): boolean {
  return !windowManager.isQuitting && !database.isClosed()
}

function scheduleForceExit(reason: string, timeoutMs = 5000): void {
  if (forceExitTimer) return
  forceExitTimer = setTimeout(() => {
    console.error(`[quit] ${reason}; forcing exit`)
    app.exit(0)
  }, timeoutMs)
  forceExitTimer.unref?.()
}

function disposeRuntimeForQuit(options: { disposeWindows: boolean; forceExit?: boolean }): void {
  windowManager.isQuitting = true

  if (updateCheckIntervalTimer) {
    clearInterval(updateCheckIntervalTimer)
    updateCheckIntervalTimer = null
  }

  crashReporter?.clearSentinel()
  notchOverlay?.dispose()
  agentSessionManager?.dispose()
  remoteSessionService.dispose()
  perfHudService.shutdown()

  if (options.disposeWindows) {
    windowManager.disposeAll()
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.destroy()
    }
  }

  wsBridge.disposeAll()
  ptyManager.dispose()
  database.close()
  if (options.forceExit !== false) {
    scheduleForceExit('shutdown did not complete within 5s')
  }
}

function persistOpenWindowConfigs(configs?: WindowConfig[]): void {
  const currentConfigs = configs ?? windowManager.getAllWindowConfigs()
  const nextConfigs =
    currentConfigs.length > 0 ? currentConfigs : (lastClosedWindowConfigs ?? currentConfigs)

  if (!configs && currentConfigs.length > 0) {
    lastClosedWindowConfigs = null
  }

  if (nextConfigs.length > 0) {
    preferencesStore.set('openWindowConfigs', JSON.stringify(nextConfigs))
  } else {
    preferencesStore.delete('openWindowConfigs')
  }
}

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
  if (!canCreateApplicationWindow()) return

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

    const gitInfo = await GitRepository.detect(resolved).unwrapOr({
      isGitRepo: false,
      repoRoot: null,
      branch: null,
      worktrees: [],
      isDirty: false,
      aheadBehind: null,
    })
    const dedupePaths = [gitInfo.repoRoot ?? resolved, ...gitInfo.worktrees.map((wt) => wt.path)]

    // Dedupe: focus existing window for this path (no confirmation needed)
    const existing = dedupePaths
      .map((dedupePath) => windowManager.getWindowForPath(dedupePath))
      .find((win) => win !== null)
    if (existing) {
      ipcCommandBridge?.grantAttachPath(existing.webContents.id, resolved)
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
      ipcCommandBridge?.grantAttachPath(win.webContents.id, resolved)
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
          click: () => {
            if (!canCreateApplicationWindow()) return
            windowManager.createWindow({
              bounds: cascadeBounds(windowManager.getLastFocusedBounds()),
            })
          },
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

  crashReporter = new CrashReporter()

  if (app.isPackaged) {
    crashReporter.init()

    // Fallback for before-quit async paths that return without clearing the sentinel (#147)
    process.on('exit', () => {
      crashReporter?.clearSentinel()
    })

    process.on('uncaughtException', (error) => {
      crashReporter?.recordCrash('uncaughtException', error)
    })

    process.on('unhandledRejection', (reason) => {
      crashReporter?.recordCrash(
        'unhandledRejection',
        reason instanceof Error ? reason : new Error(String(reason)),
      )
    })

    app.on('child-process-gone', (_event, details) => {
      if (details.reason !== 'clean-exit') {
        crashReporter?.recordCrash(
          'childProcessGone',
          new Error(`${details.type} process crashed: ${details.reason}`),
        )
      }
    })
  }

  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window)
    if (app.isPackaged) {
      window.on('focus', () => telemetryManager.onWindowFocus())

      window.webContents.on('render-process-gone', (_event, details) => {
        if (details.reason !== 'clean-exit') {
          crashReporter?.recordRendererCrash(details)
        }
      })
    }
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
    autoUpdater.allowDowngrade = false

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
      updateInstalling = false
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

      autoUpdater.quitAndInstall(true, true)
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
  await agentSessionManager.cleanupOrphans()
  windowManager.setAgentSessionManager(agentSessionManager)
  windowManager.setBrowserManager(browserManager)

  // Migrate legacy global agent prefs into Default profiles. safeStorage is
  // guaranteed to be initialized inside app.whenReady().
  // Defer to allow window creation to proceed without blocking
  setImmediate(() => profileStore.ensureDefaults())

  const keychainTokenStore = new KeychainTokenStore(preferencesStore)
  const repoConfigManager = new RepoConfigManager()
  const globalConfigManager = new GlobalConfigManager(preferencesStore, keychainTokenStore)
  const { RunConfigManager } = await import('./runConfig/RunConfigManager')
  const runConfigManager = new RunConfigManager()
  const taskTrackerManager = new TaskTrackerManager(preferencesStore, keychainTokenStore)
  const gitHubService = new GitHubService(preferencesStore, taskTrackerManager)

  if (PERF) performance.mark('app:managersReady')

  ipcCommandBridge = registerIpcHandlers(
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
    skillRegistry,
    skillInstaller,
    skillStore,
    profileStore,
    settingsExportService,
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

    ipcMain.handle('perf:disconnectTerminalClients', () => wsBridge.disconnectAllClients())

    ipcMain.handle('perf:openProject', (event, payload: { path: string }) => {
      let resolved: string
      let home: string
      try {
        resolved = realpathSync(resolve(payload.path))
        home = realpathSync(os.homedir())
      } catch {
        return // Path doesn't exist
      }
      // Constrain the renderer-supplied path to the user's home directory before
      // granting workspace access, mirroring handleCanopyUrl. Without this a
      // renderer could grant itself fs/git access to any path (e.g. `/`).
      if (resolved !== home && !resolved.startsWith(home + sep)) return
      ipcCommandBridge?.grantAttachPath(event.sender.id, resolved)
      event.sender.send('url:action', { action: 'open', path: resolved })
    })
  }

  // Status-bar perf HUD (always available, gated by user preference in renderer)
  ipcMain.handle('perf:hud:start', (event) => {
    if (!windowManager.getWindowById(BrowserWindow.fromWebContents(event.sender)?.id ?? -1)) return
    perfHudService.subscribe(event.sender)
  })
  ipcMain.handle('perf:hud:stop', (event) => {
    if (!windowManager.getWindowById(BrowserWindow.fromWebContents(event.sender)?.id ?? -1)) return
    perfHudService.unsubscribe(event.sender)
  })

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

  ipcMain.handle('app:getAboutInfo', async () => ({
    version: app.getVersion(),
    homepage: 'https://canopy.itsol.tech',
    license: await readLicenseText(),
  }))

  ipcMain.handle(
    'app:getChangelogSinceVersion',
    async (_e, { fromVersion }: { fromVersion: string }) => {
      if (typeof fromVersion !== 'string' || !semver.valid(fromVersion)) return null
      const channel = preferencesStore.get('update.channel') === 'next' ? 'next' : 'stable'
      return fetchChangelogRange(fromVersion, app.getVersion(), channel).unwrapOr(null)
    },
  )

  type TerminalStreamPauseReason = 'lock-screen' | 'suspend'
  type TerminalStreamEventReason = TerminalStreamPauseReason | 'unlock-screen' | 'resume'

  const terminalStreamPauseReasons = new Set<TerminalStreamPauseReason>()

  const broadcastTerminalStreamState = (
    state: 'paused' | 'resumed',
    reason: TerminalStreamEventReason,
  ): void => {
    const payload = {
      state,
      reason,
      pauseReasons: [...terminalStreamPauseReasons],
    }
    for (const win of BrowserWindow.getAllWindows()) {
      if (!win.isDestroyed()) win.webContents.send('terminal-stream:state', payload)
    }
  }

  const pauseTerminalStreams = (reason: TerminalStreamPauseReason): void => {
    const wasPaused = terminalStreamPauseReasons.size > 0
    terminalStreamPauseReasons.add(reason)
    if (wasPaused) return

    broadcastTerminalStreamState('paused', reason)
    wsBridge.disconnectAllClients()
  }

  const resumeTerminalStreams = (
    clearReason: TerminalStreamPauseReason,
    eventReason: Extract<TerminalStreamEventReason, 'unlock-screen' | 'resume'>,
  ): void => {
    const wasPaused = terminalStreamPauseReasons.size > 0
    terminalStreamPauseReasons.delete(clearReason)
    if (!wasPaused || terminalStreamPauseReasons.size > 0) return

    broadcastTerminalStreamState('resumed', eventReason)
  }

  const handlePowerResume = (): void => {
    const wasPaused = terminalStreamPauseReasons.size > 0
    resumeTerminalStreams('suspend', 'resume')
    if (!wasPaused) {
      wsBridge.disconnectAllClients()
    }
  }

  ipcMain.handle('terminal-stream:getState', () => ({
    state: terminalStreamPauseReasons.size > 0 ? ('paused' as const) : ('resumed' as const),
    pauseReasons: [...terminalStreamPauseReasons],
  }))

  powerMonitor.on('lock-screen', () => pauseTerminalStreams('lock-screen'))
  powerMonitor.on('suspend', () => pauseTerminalStreams('suspend'))
  powerMonitor.on('unlock-screen', () => resumeTerminalStreams('lock-screen', 'unlock-screen'))
  powerMonitor.on('resume', handlePowerResume)

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

      const crashData = crashReporter?.getCrashReport()
      if (crashData) {
        win.webContents.send('app:crashReport', crashData)
        crashReporter?.clearCrashReport()
      }

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
          startupRestore: true,
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
    if (!canCreateApplicationWindow()) return

    const url = argv.find((a) => a.startsWith('canopy://'))
    if (url) {
      handleCanopyUrl(url)
    } else {
      windowManager.createWindow({ bounds: cascadeBounds(windowManager.getLastFocusedBounds()) })
    }
  })

  app.on('activate', function () {
    if (!canCreateApplicationWindow()) return

    if (BrowserWindow.getAllWindows().length === 0)
      windowManager.createWindow({ bounds: cascadeBounds(windowManager.getLastFocusedBounds()) })
  })
})

app.on('before-quit', (event) => {
  if (database.isClosed()) return

  // During update install, let electron-updater own the window/app shutdown sequence.
  // Window configs were already saved before quitAndInstall().
  if (updateInstalling) {
    disposeRuntimeForQuit({ disposeWindows: false, forceExit: false })
    return
  }

  // Persist the exact window/project grouping before any async quit path can
  // re-enter or tear down windows.
  persistOpenWindowConfigs()

  // hasAnyActiveSession() is async, but event.preventDefault() must be called
  // synchronously. preventDefault when any tracked PTY exists, then re-decide
  // async — if nothing is actually busy, re-enter via app.quit().
  if (!windowManager.isQuitting && windowManager.getAllWindows().some((w) => !w.isDestroyed())) {
    const anyTracked = windowManager.getAllWindows().some((w) => {
      const id = w.webContents.id
      return windowManager.hasTrackedPtySessions(id)
    })
    if (anyTracked) {
      event.preventDefault()
      void windowManager.hasAnyActiveSession().then(async (activeInfo) => {
        if (!activeInfo) {
          windowManager.isQuitting = true
          app.quit()
          return
        }
        const focusedWin = BrowserWindow.getFocusedWindow() ?? windowManager.getAllWindows()[0]
        if (!focusedWin || focusedWin.isDestroyed()) {
          windowManager.isQuitting = true
          app.quit()
          return
        }
        const { response } = await dialog.showMessageBox(focusedWin, {
          type: 'warning',
          buttons: ['Quit', 'Cancel'],
          defaultId: 1,
          cancelId: 1,
          title: 'Active Sessions',
          message: 'There are active sessions running',
          detail: activeInfo,
        })
        if (response === 0) {
          windowManager.isQuitting = true
          app.quit()
        }
      })
      return
    }
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
  disposeRuntimeForQuit({ disposeWindows: true })
})

app.on('window-all-closed', () => {
  if (windowManager.isQuitting || database.isClosed()) return
  if (windowManager.shouldQuitOnLastWindowClose()) app.quit()
})
