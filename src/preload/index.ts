import { contextBridge, ipcRenderer, webUtils } from 'electron'
import type { IpcRendererEvent } from 'electron'

const api = {
  // PTY
  spawnPty: (options?: { cols?: number; rows?: number; cwd?: string }) =>
    ipcRenderer.invoke('pty:spawn', options),
  resizePty: (sessionId: string, cols: number, rows: number) =>
    ipcRenderer.invoke('pty:resize', { sessionId, cols, rows }),
  killPty: (sessionId: string) => ipcRenderer.invoke('pty:kill', { sessionId }),
  writePty: (sessionId: string, data: string) =>
    ipcRenderer.invoke('pty:write', { sessionId, data }),
  hasChildProcess: (sessionId: string) =>
    ipcRenderer.invoke('pty:hasChildProcess', { sessionId }) as Promise<boolean>,

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

  // Claude session
  updateClaudeTitle: (sessionId: string, title: string) =>
    ipcRenderer.invoke('claude:updateTitle', { sessionId, title }),

  // Auto-update
  checkForUpdates: () => ipcRenderer.invoke('app:checkForUpdates'),
  installUpdate: () => ipcRenderer.invoke('app:installUpdate'),

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
  setFocusedClaudeSession: (ptySessionId: string | null) =>
    ipcRenderer.invoke('app:setFocusedClaudeSession', { ptySessionId }),
  detachProject: (path: string) => ipcRenderer.invoke('app:detachProject', { path }),
  focusWindowForPath: (path: string) =>
    ipcRenderer.invoke('app:focusWindowForPath', { path }) as Promise<boolean>,

  // Dialog
  openFolder: () => ipcRenderer.invoke('dialog:openFolder'),

  // Git
  refreshWorkspaceGitStatus: (id: string, path: string) =>
    ipcRenderer.invoke('db:workspace:refreshGitStatus', { id, path }),

  gitDetect: (path: string) => ipcRenderer.invoke('git:detect', { path }),
  gitWorktrees: (repoRoot: string) => ipcRenderer.invoke('git:worktrees', { repoRoot }),
  gitStatus: (path: string) => ipcRenderer.invoke('git:status', { path }),
  gitWatch: (repoRoot: string) => ipcRenderer.invoke('git:watch', { repoRoot }),
  gitUnwatch: (repoRoot?: string) => ipcRenderer.invoke('git:unwatch', { repoRoot }),
  gitInit: (path: string) => ipcRenderer.invoke('git:init', { path }),

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
  gitGenerateCommitMessage: (repoRoot: string) =>
    ipcRenderer.invoke('git:generateCommitMessage', { repoRoot }),

  // Browser
  createBrowser: () => ipcRenderer.invoke('browser:create') as Promise<{ browserId: string }>,
  destroyBrowser: (browserId: string) => ipcRenderer.invoke('browser:destroy', { browserId }),
  navigateBrowser: (browserId: string, url: string) =>
    ipcRenderer.invoke('browser:navigate', { browserId, url }),
  browserBack: (browserId: string) => ipcRenderer.invoke('browser:back', { browserId }),
  browserForward: (browserId: string) => ipcRenderer.invoke('browser:forward', { browserId }),
  browserReload: (browserId: string) => ipcRenderer.invoke('browser:reload', { browserId }),
  setBrowserBounds: (
    browserId: string,
    bounds: { x: number; y: number; width: number; height: number },
  ) => ipcRenderer.invoke('browser:setBounds', { browserId, bounds }),
  setBrowserVisible: (browserId: string, visible: boolean) =>
    ipcRenderer.invoke('browser:setVisible', { browserId, visible }),
  toggleBrowserDevTools: (browserId: string, mode?: 'bottom' | 'right') =>
    ipcRenderer.invoke('browser:toggleDevTools', { browserId, mode }),
  getBrowserState: (browserId: string) => ipcRenderer.invoke('browser:getState', { browserId }),
  capturePageFull: (browserId: string) =>
    ipcRenderer.invoke('browser:capturePageFull', { browserId }) as Promise<string | null>,
  browserStartElementPick: (browserId: string) =>
    ipcRenderer.invoke('browser:startElementPick', { browserId }) as Promise<string | null>,
  browserStartRegionCapture: (browserId: string) =>
    ipcRenderer.invoke('browser:startRegionCapture', { browserId }) as Promise<string | null>,
  browserCancelPick: (browserId: string) => ipcRenderer.invoke('browser:cancelPick', { browserId }),

  onBrowserUrlChanged: (callback: (data: { browserId: string; url: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { browserId: string; url: string }): void =>
      callback(data)
    ipcRenderer.on('browser:urlChanged', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:urlChanged', handler)
    }
  },

  onBrowserTitleChanged: (callback: (data: { browserId: string; title: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { browserId: string; title: string }): void =>
      callback(data)
    ipcRenderer.on('browser:titleChanged', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:titleChanged', handler)
    }
  },

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

  onBrowserFocused: (callback: (data: { browserId: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { browserId: string }): void => callback(data)
    ipcRenderer.on('browser:focused', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:focused', handler)
    }
  },

  onBrowserLoadingChanged: (
    callback: (data: { browserId: string; isLoading: boolean }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { browserId: string; isLoading: boolean },
    ): void => callback(data)
    ipcRenderer.on('browser:loadingChanged', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:loadingChanged', handler)
    }
  },

  onBrowserLoadFailed: (
    callback: (data: {
      browserId: string
      errorCode: number
      errorDescription: string
      validatedURL: string
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: {
        browserId: string
        errorCode: number
        errorDescription: string
        validatedURL: string
      },
    ): void => callback(data)
    ipcRenderer.on('browser:loadFailed', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:loadFailed', handler)
    }
  },

  onBrowserStateChanged: (
    callback: (data: {
      browserId: string
      canGoBack: boolean
      canGoForward: boolean
      isDevToolsOpen: boolean
      devToolsMode: 'bottom' | 'right'
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: {
        browserId: string
        canGoBack: boolean
        canGoForward: boolean
        isDevToolsOpen: boolean
        devToolsMode: 'bottom' | 'right'
      },
    ): void => callback(data)
    ipcRenderer.on('browser:stateChanged', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:stateChanged', handler)
    }
  },

  // Worktree Setup
  runWorktreeSetup: (workspaceId: string, repoRoot: string, newWorktreePath: string) =>
    ipcRenderer.invoke('worktree:runSetup', { workspaceId, repoRoot, newWorktreePath }),

  // Layouts
  saveLayout: (workspaceId: string, worktreePath: string, layoutJson: string) =>
    ipcRenderer.invoke('layout:save', { workspaceId, worktreePath, layoutJson }),
  getLayout: (workspaceId: string, worktreePath: string) =>
    ipcRenderer.invoke('layout:get', { workspaceId, worktreePath }),
  getAllLayouts: (workspaceId: string) => ipcRenderer.invoke('layout:getAll', { workspaceId }),
  deleteLayout: (workspaceId: string, worktreePath: string) =>
    ipcRenderer.invoke('layout:delete', { workspaceId, worktreePath }),

  // Push events (main → renderer)
  onClaudeHookEvent: (
    callback: (data: { ptySessionId: string; event: Record<string, unknown> }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { ptySessionId: string; event: Record<string, unknown> },
    ): void => callback(data)
    ipcRenderer.on('claude:hookEvent', handler)
    return (): void => {
      ipcRenderer.removeListener('claude:hookEvent', handler)
    }
  },

  onClaudeStatusUpdate: (
    callback: (data: { ptySessionId: string; status: Record<string, unknown> }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { ptySessionId: string; status: Record<string, unknown> },
    ): void => callback(data)
    ipcRenderer.on('claude:statusUpdate', handler)
    return (): void => {
      ipcRenderer.removeListener('claude:statusUpdate', handler)
    }
  },

  onClaudeFocusSession: (callback: (data: { ptySessionId: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { ptySessionId: string }): void =>
      callback(data)
    ipcRenderer.on('claude:focusSession', handler)
    return (): void => {
      ipcRenderer.removeListener('claude:focusSession', handler)
    }
  },

  onGitChanged: (callback: (info: unknown) => void) => {
    const handler = (_event: IpcRendererEvent, info: unknown): void => callback(info)
    ipcRenderer.on('git:changed', handler)
    return (): void => {
      ipcRenderer.removeListener('git:changed', handler)
    }
  },
  onPtyExit: (
    callback: (data: { sessionId: string; exitCode: number; signal: number }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { sessionId: string; exitCode: number; signal: number },
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

  onRestoreActiveWorktree: (callback: (path: string) => void) => {
    const handler = (_event: IpcRendererEvent, path: string): void => callback(path)
    ipcRenderer.on('workspace:restoreActive', handler)
    return (): void => {
      ipcRenderer.removeListener('workspace:restoreActive', handler)
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

  // File utilities
  getPathForFile: (file: File) => webUtils.getPathForFile(file),
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
