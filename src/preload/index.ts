import { contextBridge, ipcRenderer, webUtils } from 'electron'
import type { IpcRendererEvent } from 'electron'
import type { GitInfo } from '../main/git/GitRepository'

const api = {
  // PTY
  spawnPty: (options?: { cols?: number; rows?: number; cwd?: string }) =>
    ipcRenderer.invoke('pty:spawn', options),
  resizePty: (sessionId: string, cols: number, rows: number) =>
    ipcRenderer.invoke('pty:resize', { sessionId, cols, rows }),
  killPty: (sessionId: string, killTmux?: boolean) =>
    ipcRenderer.invoke('pty:kill', { sessionId, killTmux }),
  writePty: (sessionId: string, data: string) =>
    ipcRenderer.invoke('pty:write', { sessionId, data }),
  hasChildProcess: (sessionId: string) =>
    ipcRenderer.invoke('pty:hasChildProcess', { sessionId }) as Promise<boolean>,

  // Tmux
  tmuxIsAvailable: () => ipcRenderer.invoke('tmux:isAvailable') as Promise<boolean>,
  tmuxGetVersion: () => ipcRenderer.invoke('tmux:getVersion') as Promise<string | null>,
  tmuxListSessions: () =>
    ipcRenderer.invoke('tmux:listSessions') as Promise<
      Array<{ name: string; created: number; attached: boolean; cwd: string }>
    >,
  tmuxHasSession: (name: string) =>
    ipcRenderer.invoke('tmux:hasSession', { name }) as Promise<boolean>,
  tmuxAttach: (tmuxSessionName: string, options?: { cols?: number; rows?: number }) =>
    ipcRenderer.invoke('tmux:attach', { tmuxSessionName, ...options }) as Promise<{
      sessionId: string
      wsUrl: string
    }>,
  tmuxDetach: (sessionId: string) =>
    ipcRenderer.invoke('tmux:detach', { sessionId }) as Promise<{
      tmuxSessionName?: string
    }>,
  tmuxKillSession: (name: string) => ipcRenderer.invoke('tmux:killSession', { name }),
  tmuxRenameSession: (oldName: string, newName: string) =>
    ipcRenderer.invoke('tmux:renameSession', { oldName, newName }),

  // Workspaces
  listWorkspaces: (limit?: number) => ipcRenderer.invoke('db:workspace:list', { limit }),
  getWorkspace: (id: string) => ipcRenderer.invoke('db:workspace:get', { id }),
  getWorkspaceByPath: (path: string) => ipcRenderer.invoke('db:workspace:getByPath', { path }),
  upsertWorkspace: (workspace: { path: string; name: string; isGitRepo: boolean }) =>
    ipcRenderer.invoke('db:workspace:upsert', workspace),
  removeWorkspace: (id: string) => ipcRenderer.invoke('db:workspace:remove', { id }),
  touchWorkspace: (id: string) => ipcRenderer.invoke('db:workspace:touch', { id }),

  // Preferences
  getPref: (key: string) => ipcRenderer.invoke('db:prefs:get', { key }),
  setPref: (key: string, value: string) => ipcRenderer.invoke('db:prefs:set', { key, value }),
  getAllPrefs: () => ipcRenderer.invoke('db:prefs:getAll'),
  deletePref: (key: string) => ipcRenderer.invoke('db:prefs:delete', { key }),

  // Notch overlay
  setNotchEnabled: (enabled: boolean) => ipcRenderer.send('notch:setEnabled', { enabled }),

  // Environment / Dependencies
  checkDependencies: (tools: string[]) =>
    ipcRenderer.invoke('env:checkDependencies', { tools }) as Promise<{
      results: Record<string, { found: boolean; path?: string }>
      platform: string
    }>,

  // Tools
  listTools: () => ipcRenderer.invoke('tools:list'),
  getTool: (id: string) => ipcRenderer.invoke('tools:get', { id }),
  checkToolAvailability: () => ipcRenderer.invoke('tools:checkAvailability'),
  spawnTool: (
    toolId: string,
    worktreePath: string,
    options?: {
      cols?: number
      rows?: number
      workspaceName?: string
      branch?: string
      resumeSessionId?: string
    },
  ) => ipcRenderer.invoke('tool:spawn', { toolId, worktreePath, ...options }),
  addCustomTool: (tool: {
    id: string
    name: string
    command: string
    args?: string[]
    icon?: string
    category?: string
  }) => ipcRenderer.invoke('tools:addCustom', tool),
  removeCustomTool: (id: string) => ipcRenderer.invoke('tools:removeCustom', { id }),
  updateCustomTool: (
    id: string,
    changes: {
      name?: string
      command?: string
      args?: string[]
      icon?: string
      category?: string
    },
  ) => ipcRenderer.invoke('tools:updateCustom', { id, changes }),
  // Agent session
  updateAgentTitle: (sessionId: string, title: string) =>
    ipcRenderer.invoke('agent:updateTitle', { sessionId, title }),

  // Auto-update
  checkForUpdates: () => ipcRenderer.invoke('app:checkForUpdates'),
  installUpdate: () => ipcRenderer.invoke('app:installUpdate'),
  setUpdateChannel: (channel: string) => ipcRenderer.invoke('app:setUpdateChannel', channel),
  setAutoUpdate: (enabled: boolean) => ipcRenderer.invoke('app:setAutoUpdate', enabled),

  onUpdateAvailable: (callback: (data: { version: string; releaseNotes?: string }) => void) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { version: string; releaseNotes?: string },
    ): void => callback(data)
    ipcRenderer.on('update:available', handler)
    return (): void => {
      ipcRenderer.removeListener('update:available', handler)
    }
  },

  onUpdateProgress: (
    callback: (data: {
      percent: number
      bytesPerSecond: number
      transferred: number
      total: number
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { percent: number; bytesPerSecond: number; transferred: number; total: number },
    ): void => callback(data)
    ipcRenderer.on('update:progress', handler)
    return (): void => {
      ipcRenderer.removeListener('update:progress', handler)
    }
  },

  onUpdateDownloaded: (callback: (data: { version: string; releaseNotes?: string }) => void) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { version: string; releaseNotes?: string },
    ): void => callback(data)
    ipcRenderer.on('update:downloaded', handler)
    return (): void => {
      ipcRenderer.removeListener('update:downloaded', handler)
    }
  },

  onUpdateNotAvailable: (callback: () => void) => {
    const handler = (): void => callback()
    ipcRenderer.on('update:not-available', handler)
    return (): void => {
      ipcRenderer.removeListener('update:not-available', handler)
    }
  },

  onUpdateError: (callback: (data: { message: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { message: string }): void => callback(data)
    ipcRenderer.on('update:error', handler)
    return (): void => {
      ipcRenderer.removeListener('update:error', handler)
    }
  },

  onUpdateInstalling: (callback: () => void) => {
    const handler = (): void => callback()
    ipcRenderer.on('update:installing', handler)
    return (): void => {
      ipcRenderer.removeListener('update:installing', handler)
    }
  },

  // Onboarding
  getOnboardingCompleted: () => ipcRenderer.invoke('onboarding:getCompleted') as Promise<string[]>,
  completeOnboarding: (stepIds: string[], appVersion: string) =>
    ipcRenderer.invoke('onboarding:complete', { stepIds, appVersion }),
  resetOnboarding: () => ipcRenderer.invoke('onboarding:reset'),

  onShowOnboarding: (
    callback: (data: { mode: 'first-launch' | 'upgrade'; fromVersion?: string }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { mode: 'first-launch' | 'upgrade'; fromVersion?: string },
    ): void => callback(data)
    ipcRenderer.on('app:showOnboarding', handler)
    return (): void => {
      ipcRenderer.removeListener('app:showOnboarding', handler)
    }
  },

  // Changelog
  getChangelogSinceVersion: (fromVersion: string) =>
    ipcRenderer.invoke('app:getChangelogSinceVersion', { fromVersion }),

  onShowChangelog: (callback: (data: { fromVersion: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { fromVersion: string }): void =>
      callback(data)
    ipcRenderer.on('app:showChangelog', handler)
    return (): void => {
      ipcRenderer.removeListener('app:showChangelog', handler)
    }
  },

  // About
  getAboutInfo: () => ipcRenderer.invoke('app:getAboutInfo'),
  openExternal: (url: string) => ipcRenderer.invoke('app:openExternal', { url }),
  openThirdPartyNotices: () => ipcRenderer.invoke('app:openThirdPartyNotices'),
  quit: () => ipcRenderer.invoke('app:quit'),

  // App / Shell
  getHomedir: () => ipcRenderer.invoke('app:homedir') as Promise<string>,
  showInFolder: (path: string) => ipcRenderer.invoke('app:showInFolder', { path }),
  newWindow: () => ipcRenderer.invoke('app:newWindow'),
  setWorkspacePath: (path: string) => ipcRenderer.invoke('app:setWorkspacePath', { path }),
  setActiveWorktree: (path: string) => ipcRenderer.invoke('app:setActiveWorktree', { path }),
  setFocusedAgentSession: (ptySessionId: string | null) =>
    ipcRenderer.invoke('app:setFocusedAgentSession', { ptySessionId }),
  detachProject: (path: string) => ipcRenderer.invoke('app:detachProject', { path }),
  focusWindowForPath: (path: string) =>
    ipcRenderer.invoke('app:focusWindowForPath', { path }) as Promise<boolean>,
  focusRendererWebContents: () => ipcRenderer.invoke('app:focusRendererWebContents'),

  // Dialog
  openFolder: () => ipcRenderer.invoke('dialog:openFolder'),

  // Git
  refreshWorkspaceGitStatus: (id: string, path: string) =>
    ipcRenderer.invoke('db:workspace:refreshGitStatus', { id, path }),

  gitDetect: (path: string) => ipcRenderer.invoke('git:detect', { path }),
  gitWorktrees: (repoRoot: string) => ipcRenderer.invoke('git:worktrees', { repoRoot }),
  gitStatus: (path: string) => ipcRenderer.invoke('git:status', { path }),
  gitWatch: (repoRoot: string, snapshot?: GitInfo) =>
    ipcRenderer.invoke('git:watch', { repoRoot, snapshot }),
  gitUnwatch: (repoRoot?: string) => ipcRenderer.invoke('git:unwatch', { repoRoot }),
  gitInit: (path: string) => ipcRenderer.invoke('git:init', { path }),

  // File Tree Watcher
  watchFiles: (repoRoot: string) => ipcRenderer.invoke('files:watch', { repoRoot }),
  unwatchFiles: () => ipcRenderer.invoke('files:unwatch'),
  updateFileIgnorePatterns: (patterns: string[]) =>
    ipcRenderer.invoke('files:updateIgnorePatterns', { patterns }),
  getDefaultFileIgnorePatterns: () =>
    ipcRenderer.invoke('files:getDefaultIgnorePatterns') as Promise<string[]>,

  // Git Operations
  gitCommit: (repoRoot: string, message: string, stageAll?: boolean) =>
    ipcRenderer.invoke('git:commit', { repoRoot, message, stageAll }),
  gitPush: (repoRoot: string) => ipcRenderer.invoke('git:push', { repoRoot }),
  gitPull: (repoRoot: string, rebase: boolean) =>
    ipcRenderer.invoke('git:pull', { repoRoot, rebase }),
  gitFetch: (repoRoot: string) => ipcRenderer.invoke('git:fetch', { repoRoot }),
  gitFetchAll: (repoRoot: string) => ipcRenderer.invoke('git:fetchAll', { repoRoot }),
  gitStash: (repoRoot: string) => ipcRenderer.invoke('git:stash', { repoRoot }),
  gitStashPop: (repoRoot: string) => ipcRenderer.invoke('git:stashPop', { repoRoot }),
  gitBranches: (repoRoot: string) => ipcRenderer.invoke('git:branches', { repoRoot }),
  gitBranchCreate: (repoRoot: string, name: string, baseBranch: string) =>
    ipcRenderer.invoke('git:branchCreate', { repoRoot, name, baseBranch }),
  gitCheckout: (repoRoot: string, branch: string) =>
    ipcRenderer.invoke('git:checkout', { repoRoot, branch }),
  gitBranchDelete: (repoRoot: string, name: string, force: boolean) =>
    ipcRenderer.invoke('git:branchDelete', { repoRoot, name, force }),
  gitBranchDeleteRemote: (repoRoot: string, remote: string, name: string) =>
    ipcRenderer.invoke('git:branchDeleteRemote', { repoRoot, remote, name }),
  gitPushInfo: (repoRoot: string) => ipcRenderer.invoke('git:pushInfo', { repoRoot }),
  gitBranchMerged: (repoRoot: string, branch: string) =>
    ipcRenderer.invoke('git:branchMerged', { repoRoot, branch }),
  gitWorktreeAdd: (repoRoot: string, path: string, branch: string, baseBranch: string) =>
    ipcRenderer.invoke('git:worktreeAdd', { repoRoot, path, branch, baseBranch }),
  gitWorktreeRemove: (repoRoot: string, path: string, force: boolean) =>
    ipcRenderer.invoke('git:worktreeRemove', { repoRoot, path, force }),
  gitUnmergedCommits: (repoRoot: string, branch: string) =>
    ipcRenderer.invoke('git:unmergedCommits', { repoRoot, branch }),
  gitStatusPorcelain: (repoRoot: string, worktreePath?: string) =>
    ipcRenderer.invoke('git:statusPorcelain', { repoRoot, worktreePath }),
  gitDiff: (repoRoot: string) => ipcRenderer.invoke('git:diff', { repoRoot }),
  gitDiffFile: (repoRoot: string, filePath: string) =>
    ipcRenderer.invoke('git:diffFile', { repoRoot, filePath }),
  gitStageFile: (repoRoot: string, filePath: string) =>
    ipcRenderer.invoke('git:stageFile', { repoRoot, filePath }),
  gitRevertFile: (repoRoot: string, filePath: string) =>
    ipcRenderer.invoke('git:revertFile', { repoRoot, filePath }),
  gitGenerateCommitMessage: (repoRoot: string) =>
    ipcRenderer.invoke('git:generateCommitMessage', { repoRoot }),

  // Browser (<webview> management)
  setupBrowserWebview: (browserId: string, webContentsId: number) =>
    ipcRenderer.invoke('browser:setup', { browserId, webContentsId }),
  teardownBrowserWebview: (browserId: string) =>
    ipcRenderer.invoke('browser:teardown', { browserId }),
  openBrowserDevTools: (browserId: string) =>
    ipcRenderer.invoke('browser:openDevTools', { browserId }),
  closeBrowserDevTools: (browserId: string) =>
    ipcRenderer.invoke('browser:closeDevTools', { browserId }),
  setBrowserDevToolsBounds: (
    browserId: string,
    bounds: { x: number; y: number; width: number; height: number },
  ) => ipcRenderer.invoke('browser:setDevToolsBounds', { browserId, bounds }),
  setBrowserDeviceEmulation: (
    browserId: string,
    device: { width: number; height: number; scaleFactor: number; mobile: boolean } | null,
  ) => ipcRenderer.invoke('browser:setDeviceEmulation', { browserId, device }),
  setBrowserBackgroundThrottling: (browserId: string, allowed: boolean) =>
    ipcRenderer.invoke('browser:setBackgroundThrottling', { browserId, allowed }),
  saveBrowserCapture: (buffer: ArrayBuffer) =>
    ipcRenderer.invoke('browser:saveCaptureFile', {
      buffer: Buffer.from(buffer),
    }) as Promise<string>,

  // Credential autofill (isolated world)
  fillBrowserCredential: (browserId: string, username: string, password: string) =>
    ipcRenderer.invoke('browser:fillCredential', { browserId, username, password }),

  // Credentials
  getCredentials: (domain: string) =>
    ipcRenderer.invoke('credentials:getForDomain', { domain }) as Promise<
      Array<{ id: string; domain: string; username: string; title: string }>
    >,
  saveCredential: (domain: string, username: string, password: string, title?: string) =>
    ipcRenderer.invoke('credentials:save', { domain, username, password, title }),
  getCredentialDecrypted: (id: string, domain: string) =>
    ipcRenderer.invoke('credentials:getDecrypted', { id, domain }) as Promise<{
      id: string
      username: string
      password: string
    } | null>,
  deleteCredential: (id: string) => ipcRenderer.invoke('credentials:delete', { id }),
  listCredentials: () =>
    ipcRenderer.invoke('credentials:getAll') as Promise<
      Array<{ id: string; domain: string; username: string }>
    >,

  // Browser push events (main → renderer, still needed for favicon + focus)
  onBrowserFaviconChanged: (
    callback: (data: { browserId: string; favicon: string | null }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { browserId: string; favicon: string | null },
    ): void => callback(data)
    ipcRenderer.on('browser:faviconChanged', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:faviconChanged', handler)
    }
  },

  onBrowserDevToolsOpened: (callback: (data: { browserId: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { browserId: string }): void => callback(data)
    ipcRenderer.on('browser:devToolsOpened', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:devToolsOpened', handler)
    }
  },

  onBrowserFocused: (callback: (data: { browserId: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { browserId: string }): void => callback(data)
    ipcRenderer.on('browser:focused', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:focused', handler)
    }
  },

  // Worktree Setup
  runWorktreeSetup: (workspaceId: string, repoRoot: string, newWorktreePath: string) =>
    ipcRenderer.invoke('worktree:runSetup', { workspaceId, repoRoot, newWorktreePath }),
  abortWorktreeSetup: () => ipcRenderer.send('worktree:abortSetup'),

  // Layouts
  saveLayout: (workspaceId: string, worktreePath: string, layoutJson: string) =>
    ipcRenderer.invoke('layout:save', { workspaceId, worktreePath, layoutJson }),
  getLayout: (workspaceId: string, worktreePath: string) =>
    ipcRenderer.invoke('layout:get', { workspaceId, worktreePath }),
  getAllLayouts: (workspaceId: string) => ipcRenderer.invoke('layout:getAll', { workspaceId }),
  deleteLayout: (workspaceId: string, worktreePath: string) =>
    ipcRenderer.invoke('layout:delete', { workspaceId, worktreePath }),

  // Push events (main → renderer)
  onAgentHookEvent: (
    callback: (data: {
      ptySessionId: string
      agentType: string
      event: Record<string, unknown>
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { ptySessionId: string; agentType: string; event: Record<string, unknown> },
    ): void => callback(data)
    ipcRenderer.on('agent:hookEvent', handler)
    return (): void => {
      ipcRenderer.removeListener('agent:hookEvent', handler)
    }
  },

  onAgentStatusUpdate: (
    callback: (data: {
      ptySessionId: string
      agentType: string
      status: Record<string, unknown>
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { ptySessionId: string; agentType: string; status: Record<string, unknown> },
    ): void => callback(data)
    ipcRenderer.on('agent:statusUpdate', handler)
    return (): void => {
      ipcRenderer.removeListener('agent:statusUpdate', handler)
    }
  },

  onAgentFocusSession: (callback: (data: { ptySessionId: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { ptySessionId: string }): void =>
      callback(data)
    ipcRenderer.on('agent:focusSession', handler)
    return (): void => {
      ipcRenderer.removeListener('agent:focusSession', handler)
    }
  },

  onGitChanged: (callback: (info: unknown) => void) => {
    const handler = (_event: IpcRendererEvent, info: unknown): void => callback(info)
    ipcRenderer.on('git:changed', handler)
    return (): void => {
      ipcRenderer.removeListener('git:changed', handler)
    }
  },
  onFilesChanged: (
    callback: (payload: {
      repoRoot: string
      events: { type: 'add' | 'change' | 'unlink'; path: string }[]
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      payload: {
        repoRoot: string
        events: { type: 'add' | 'change' | 'unlink'; path: string }[]
      },
    ): void => callback(payload)
    ipcRenderer.on('files:changed', handler)
    return (): void => {
      ipcRenderer.removeListener('files:changed', handler)
    }
  },
  onToolsChanged: (callback: (tools: unknown[]) => void) => {
    const handler = (_event: IpcRendererEvent, tools: unknown[]): void => callback(tools)
    ipcRenderer.on('tools:changed', handler)
    return (): void => {
      ipcRenderer.removeListener('tools:changed', handler)
    }
  },
  onPtyExit: (
    callback: (data: {
      sessionId: string
      exitCode: number
      signal: number
      tmuxSessionName?: string
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { sessionId: string; exitCode: number; signal: number; tmuxSessionName?: string },
    ): void => callback(data)
    ipcRenderer.on('pty:exit', handler)
    return (): void => {
      ipcRenderer.removeListener('pty:exit', handler)
    }
  },

  onWorktreeSetupProgress: (
    callback: (data: {
      actionIndex: number
      totalActions: number
      label: string
      status: 'running' | 'done' | 'error'
      output?: string
      error?: string
      outputChunk?: string
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: {
        actionIndex: number
        totalActions: number
        label: string
        status: 'running' | 'done' | 'error'
        output?: string
        error?: string
        outputChunk?: string
      },
    ): void => callback(data)
    ipcRenderer.on('worktree:setupProgress', handler)
    return (): void => {
      ipcRenderer.removeListener('worktree:setupProgress', handler)
    }
  },

  onUrlAction: (
    callback: (data: { action: string; path: string; tool?: string; worktree?: string }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { action: string; path: string; tool?: string; worktree?: string },
    ): void => callback(data)
    ipcRenderer.on('url:action', handler)
    return (): void => {
      ipcRenderer.removeListener('url:action', handler)
    }
  },

  onRestoreWindow: (callback: (data: { paths: string[]; activeWorktreePath?: string }) => void) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { paths: string[]; activeWorktreePath?: string },
    ): void => callback(data)
    ipcRenderer.on('workspace:restoreWindow', handler)
    return (): void => {
      ipcRenderer.removeListener('workspace:restoreWindow', handler)
    }
  },

  // Menu events (from native menu)
  onMenuShowAbout: (callback: () => void) => {
    const handler = (): void => callback()
    ipcRenderer.on('menu:showAbout', handler)
    return (): void => {
      ipcRenderer.removeListener('menu:showAbout', handler)
    }
  },
  onMenuShowPreferences: (callback: () => void) => {
    const handler = (): void => callback()
    ipcRenderer.on('menu:showPreferences', handler)
    return (): void => {
      ipcRenderer.removeListener('menu:showPreferences', handler)
    }
  },

  // Filesystem
  readDir: (dirPath: string) => ipcRenderer.invoke('fs:readDir', { dirPath }),
  readFile: (filePath: string, maxBytes?: number) =>
    ipcRenderer.invoke('fs:readFile', { filePath, maxBytes }),

  // Repo Config
  repoConfigLoad: (repoRoot: string) => ipcRenderer.invoke('repoConfig:load', { repoRoot }),
  repoConfigSave: (repoRoot: string, config: unknown) =>
    ipcRenderer.invoke('repoConfig:save', { repoRoot, config }),
  repoConfigExists: (repoRoot: string) =>
    ipcRenderer.invoke('repoConfig:exists', { repoRoot }) as Promise<boolean>,
  repoConfigInit: (repoRoot: string) => ipcRenderer.invoke('repoConfig:init', { repoRoot }),

  // Global Config
  globalConfigLoad: () => ipcRenderer.invoke('globalConfig:load'),
  globalConfigSave: (config: unknown) => ipcRenderer.invoke('globalConfig:save', { config }),
  globalConfigExists: () => ipcRenderer.invoke('globalConfig:exists') as Promise<boolean>,

  // Resolved Config (merged global + repo)
  trackerResolvedConfig: (repoRoot?: string) =>
    ipcRenderer.invoke('tracker:resolvedConfig', { repoRoot }),

  // Config-based tracker methods
  trackerConfigFetchBoards: (repoRoot?: string, trackerId?: string) =>
    ipcRenderer.invoke('trackerConfig:fetchBoards', { repoRoot, trackerId }),
  trackerConfigFetchStatuses: (repoRoot?: string, trackerId?: string, boardId?: string) =>
    ipcRenderer.invoke('trackerConfig:fetchStatuses', { repoRoot, trackerId, boardId }),
  trackerConfigFetchTasks: (
    repoRoot?: string,
    trackerId?: string,
    params?: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
  ) => ipcRenderer.invoke('trackerConfig:fetchTasks', { repoRoot, trackerId, ...params }),
  trackerConfigGetCurrentUser: (repoRoot?: string, trackerId?: string) =>
    ipcRenderer.invoke('trackerConfig:getCurrentUser', { repoRoot, trackerId }),

  // Keychain
  keychainHasCredentials: (provider: string, baseUrl: string) =>
    ipcRenderer.invoke('keychain:hasCredentials', { provider, baseUrl }) as Promise<boolean>,
  keychainSetCredentials: (provider: string, baseUrl: string, token: string, username?: string) =>
    ipcRenderer.invoke('keychain:setCredentials', { provider, baseUrl, token, username }),
  keychainDeleteCredentials: (provider: string, baseUrl: string) =>
    ipcRenderer.invoke('keychain:deleteCredentials', { provider, baseUrl }),
  keychainGetCredentials: (provider: string, baseUrl: string) =>
    ipcRenderer.invoke('keychain:getCredentials', { provider, baseUrl }) as Promise<{
      username?: string
      hasToken: boolean
    } | null>,

  // Task Tracker
  taskTrackerGetConnections: () => ipcRenderer.invoke('taskTracker:getConnections'),
  taskTrackerAddConnection: (connection: {
    provider: string
    name: string
    baseUrl: string
    projectKey: string
    boardId?: string
    username?: string
    token: string
  }) => ipcRenderer.invoke('taskTracker:addConnection', connection),
  taskTrackerRemoveConnection: (connectionId: string) =>
    ipcRenderer.invoke('taskTracker:removeConnection', { connectionId }),
  taskTrackerUpdateConnection: (
    connectionId: string,
    updates: { name?: string; baseUrl?: string; username?: string; token?: string },
  ) => ipcRenderer.invoke('taskTracker:updateConnection', { connectionId, ...updates }),
  taskTrackerTestConnection: (connectionId: string) =>
    ipcRenderer.invoke('taskTracker:testConnection', { connectionId }),
  taskTrackerTestNewConnection: (connection: {
    provider: string
    name: string
    baseUrl: string
    projectKey: string
    boardId?: string
    username?: string
    token: string
  }) => ipcRenderer.invoke('taskTracker:testNewConnection', connection),
  taskTrackerFetchBoards: (connectionId: string, repoRoot?: string) =>
    ipcRenderer.invoke('taskTracker:fetchBoards', { connectionId, repoRoot }),
  taskTrackerFetchBoardsForNew: (connection: {
    provider: string
    name: string
    baseUrl: string
    projectKey?: string
    username?: string
    token: string
  }) => ipcRenderer.invoke('taskTracker:fetchBoardsForNew', connection),
  taskTrackerFetchStatuses: (connectionId: string, boardId?: string, repoRoot?: string) =>
    ipcRenderer.invoke('taskTracker:fetchStatuses', { connectionId, boardId, repoRoot }),
  taskTrackerFetchTasks: (
    connectionId: string,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string; repoRoot?: string },
  ) => ipcRenderer.invoke('taskTracker:fetchTasks', { connectionId, ...params }),
  taskTrackerGetCurrentSprint: (connectionId: string, boardId?: string, repoRoot?: string) =>
    ipcRenderer.invoke('taskTracker:getCurrentSprint', { connectionId, boardId, repoRoot }),
  taskTrackerGetCurrentUser: (connectionId: string) =>
    ipcRenderer.invoke('taskTracker:getCurrentUser', { connectionId }) as Promise<string>,
  taskTrackerFetchTaskComments: (connectionId: string, taskKey: string, repoRoot?: string) =>
    ipcRenderer.invoke('taskTracker:fetchTaskComments', { connectionId, taskKey, repoRoot }),
  taskTrackerFetchTaskAttachments: (connectionId: string, taskKey: string) =>
    ipcRenderer.invoke('taskTracker:fetchTaskAttachments', { connectionId, taskKey }),
  taskTrackerDownloadAttachment: (connectionId: string, url: string, filename: string) =>
    ipcRenderer.invoke('taskTracker:downloadAttachment', {
      connectionId,
      url,
      filename,
    }) as Promise<string>,
  taskTrackerCleanupAttachments: (filePaths: string[]) =>
    ipcRenderer.invoke('taskTracker:cleanupAttachments', { filePaths }),
  taskTrackerResolveBranchName: (
    connectionId: string,
    task: { key: string; type: string; [k: string]: unknown },
    boardId?: string,
    branchType?: string,
    repoRoot?: string,
  ) =>
    ipcRenderer.invoke('taskTracker:resolveBranchName', {
      connectionId,
      task,
      boardId,
      branchType,
      repoRoot,
    }),
  taskTrackerResolveBranchType: (
    taskType: string,
    connectionId?: string,
    boardId?: string,
    repoRoot?: string,
  ) =>
    ipcRenderer.invoke('taskTracker:resolveBranchType', {
      taskType,
      connectionId,
      boardId,
      repoRoot,
    }) as Promise<{
      defaultType: string
      options: string[]
      hasBranchType: boolean
    }>,
  taskTrackerRenderBranchPreview: (template: string, customVars?: Record<string, string>) =>
    ipcRenderer.invoke('taskTracker:renderBranchPreview', { template, customVars }),
  taskTrackerGetAvailablePlaceholders: (customVars?: Record<string, string>) =>
    ipcRenderer.invoke('taskTracker:getAvailablePlaceholders', { customVars }),
  taskTrackerValidateTemplate: (template: string) =>
    ipcRenderer.invoke('taskTracker:validateTemplate', { template }),
  taskTrackerFindTaskByKey: (taskKey: string) =>
    ipcRenderer.invoke('taskTracker:findTaskByKey', { taskKey }),
  taskTrackerResolvePRPreview: (
    taskKey: string,
    connectionId?: string,
    boardId?: string,
    repoRoot?: string,
  ) =>
    ipcRenderer.invoke('taskTracker:resolvePRPreview', {
      taskKey,
      connectionId,
      boardId,
      repoRoot,
    }) as Promise<{ title: string; targetBranch: string }>,
  taskTrackerCreatePR: (
    repoRoot: string,
    task: { key: string; [k: string]: unknown },
    sourceBranch: string,
    connectionId?: string,
    boardId?: string,
  ) =>
    ipcRenderer.invoke('taskTracker:createPR', {
      repoRoot,
      task,
      sourceBranch,
      connectionId,
      boardId,
    }),

  taskTrackerFindPR: (repoRoot: string, branch: string) =>
    ipcRenderer.invoke('taskTracker:findPR', { repoRoot, branch }) as Promise<string | null>,

  // GitHub PR features
  githubFetchBranchPRs: (repoRoot: string) =>
    ipcRenderer.invoke('github:fetchBranchPRs', { repoRoot }),
  githubGetRepoInfo: (repoRoot: string) => ipcRenderer.invoke('github:getRepoInfo', { repoRoot }),
  githubCreatePR: (
    repoRoot: string,
    params: { title: string; body: string; baseRefName: string; draft: boolean },
  ) => ipcRenderer.invoke('github:createPR', { repoRoot, ...params }),
  githubGetRepoIdentifier: (repoRoot: string) =>
    ipcRenderer.invoke('github:getRepoIdentifier', { repoRoot }),

  // Performance diagnostics (only active when CANOPY_PERF=1)
  ...(process.env.CANOPY_PERF === '1'
    ? {
        perfDiagnostics: () => ipcRenderer.invoke('perf:diagnostics'),
        perfIpcLog: () => ipcRenderer.invoke('perf:ipcLog'),
      }
    : {}),

  // File utilities
  getPathForFile: (file: File) => webUtils.getPathForFile(file),

  // Platform
  platform: process.platform,
}

if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('api', api)
  } catch (error) {
    console.error(error)
  }
} else {
  // @ts-ignore (define in dts)
  window.api = api
}
