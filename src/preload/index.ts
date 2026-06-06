import { contextBridge, ipcRenderer, webUtils } from 'electron'
import type { IpcRendererEvent } from 'electron'
import type { GitInfo } from '../main/git/GitRepository'
import type { GitRefreshFlags } from '../main/git/GitWatcher'
import type { ToolDefinition } from '../main/db/types'
import type { CanopySkill } from '../main/skills/types'
import type { RemoteSessionStatus } from '../main/remote/types'
import type { AgentProfileMasked, ProfileInput } from '../main/profiles/types'
import type { AgentType } from '../main/agents/types'
import type {
  AppStateSnapshot,
  CloseWarningResult,
  CloseWarningTarget,
  EditorFileLoadResult,
  EditorFileSaveResult,
  TabCloseAllPreflightResult,
  TabClosePreflightResult,
} from '../main/commands/types'

const api = {
  // PTY
  resizePty: (sessionId: string, cols: number, rows: number) =>
    ipcRenderer.invoke('pty:resize', { sessionId, cols, rows }),
  killPty: (sessionId: string, killTmux?: boolean) =>
    ipcRenderer.invoke('pty:kill', { sessionId, killTmux }),
  writePty: (sessionId: string, data: string) =>
    ipcRenderer.invoke('pty:write', { sessionId, data }),
  getPtyDimensions: (sessionId: string) =>
    ipcRenderer.invoke('pty:getDimensions', { sessionId }) as Promise<{
      cols: number
      rows: number
    } | null>,

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
  removeWorkspace: (id: string) => ipcRenderer.invoke('db:workspace:remove', { id }),
  workspaceRestoreWindow: (payload: {
    paths: string[]
    activeWorktreePath?: string
    removedPaths?: string[]
  }) => ipcRenderer.invoke('workspace:command:restoreWindow', payload),
  workspaceAttachProject: (path: string) =>
    ipcRenderer.invoke('workspace:command:attachProject', { path }),
  workspaceDetachProject: (path: string) =>
    ipcRenderer.invoke('workspace:command:detachProject', { path }),
  workspaceSelectWorktree: (path: string) =>
    ipcRenderer.invoke('workspace:command:selectWorktree', { path }),
  workspaceInitGitRepo: (path: string) =>
    ipcRenderer.invoke('workspace:command:initGitRepo', { path }),
  getAppState: () => ipcRenderer.invoke('app:getState') as Promise<AppStateSnapshot>,
  getStartupRestoreState: () =>
    ipcRenderer.invoke('app:getStartupRestoreState') as Promise<{ restoring: boolean }>,
  completeStartupRestore: () => ipcRenderer.invoke('app:completeStartupRestore') as Promise<void>,
  onAppStateChanged: (callback: (snapshot: AppStateSnapshot) => void) => {
    const handler = (_event: IpcRendererEvent, snapshot: AppStateSnapshot): void =>
      callback(snapshot)
    ipcRenderer.on('app:stateChanged', handler)
    return (): void => {
      ipcRenderer.removeListener('app:stateChanged', handler)
    }
  },
  fileTreeReadDir: (dirPath: string) => ipcRenderer.invoke('fileTree:readDir', { dirPath }),
  fileTreeCreateFile: (filePath: string) => ipcRenderer.invoke('fileTree:createFile', { filePath }),
  fileTreeCreateDirectory: (dirPath: string) =>
    ipcRenderer.invoke('fileTree:createDirectory', { dirPath }),
  fileTreeGetGitStatus: (repoRoot: string, worktreePath: string) =>
    ipcRenderer.invoke('fileTree:getGitStatus', { repoRoot, worktreePath }) as Promise<{
      statuses: Record<string, string>
      changedDirs: string[]
      affectedPaths: string[]
    }>,

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
  tabOpenTool: (
    toolId: string,
    worktreePath: string,
    options?: {
      initialUrl?: string
      profileId?: string
      workspaceName?: string
      branch?: string
    },
  ) =>
    ipcRenderer.invoke('tab:command:openTool', {
      toolId,
      worktreePath,
      options,
    }),
  tabOpenDiff: (worktreePath: string) =>
    ipcRenderer.invoke('tab:command:openDiff', {
      worktreePath,
    }),
  tabOpenSessionTab: (worktreePath: string, name: string, sessionId: string) =>
    ipcRenderer.invoke('tab:command:openSessionTab', {
      worktreePath,
      name,
      sessionId,
    }),
  tabOpenEditorFile: (worktreePath: string, filePath: string) =>
    ipcRenderer.invoke('tab:command:openEditorFile', {
      worktreePath,
      filePath,
    }),
  tabDetachEditorFile: (worktreePath: string, paneId: string, filePath: string) =>
    ipcRenderer.invoke('tab:command:detachEditorFile', {
      worktreePath,
      paneId,
      filePath,
    }),
  tabCloseEditorFile: (worktreePath: string, paneId: string, filePath: string) =>
    ipcRenderer.invoke('tab:command:closeEditorFile', {
      worktreePath,
      paneId,
      filePath,
    }),
  tabPrepareCloseEditorFile: (worktreePath: string, paneId: string, filePath: string) =>
    ipcRenderer.invoke('tab:command:prepareCloseEditorFile', {
      worktreePath,
      paneId,
      filePath,
    }) as Promise<TabClosePreflightResult>,
  tabMoveEditorFile: (worktreePath: string, paneId: string, filePath: string, toIndex: number) =>
    ipcRenderer.invoke('tab:command:moveEditorFile', {
      worktreePath,
      paneId,
      filePath,
      toIndex,
    }),
  tabMoveEditorFileBetweenPanes: (
    worktreePath: string,
    sourcePaneId: string,
    targetPaneId: string,
    filePath: string,
    toIndex: number,
  ) =>
    ipcRenderer.invoke('tab:command:moveEditorFileBetweenPanes', {
      worktreePath,
      sourcePaneId,
      targetPaneId,
      filePath,
      toIndex,
    }),
  tabSetActiveEditorFile: (worktreePath: string, paneId: string, filePath: string) =>
    ipcRenderer.invoke('tab:command:setActiveEditorFile', {
      worktreePath,
      paneId,
      filePath,
    }),
  tabUpdateEditorFileState: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    patch: unknown,
  ) =>
    ipcRenderer.invoke('tab:command:updateEditorFileState', {
      worktreePath,
      paneId,
      filePath,
      patch,
    }),
  tabLoadEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    options?: { maxBytes?: number },
  ) =>
    ipcRenderer.invoke('tab:command:loadEditorFile', {
      worktreePath,
      paneId,
      filePath,
      options,
    }) as Promise<EditorFileLoadResult>,
  tabSaveEditorFile: (
    worktreePath: string,
    paneId: string,
    filePath: string,
    options: {
      content: string
      fileLineEnding?: 'LF' | 'CRLF'
      expectedMtimeMs?: number
    },
  ) =>
    ipcRenderer.invoke('tab:command:saveEditorFile', {
      worktreePath,
      paneId,
      filePath,
      options,
    }) as Promise<EditorFileSaveResult>,
  tabUpdatePaneTitle: (worktreePath: string, sessionId: string, title: string) =>
    ipcRenderer.invoke('tab:command:updatePaneTitle', {
      worktreePath,
      sessionId,
      title,
    }),
  tabUpdatePaneUrl: (worktreePath: string, sessionId: string, url: string) =>
    ipcRenderer.invoke('tab:command:updatePaneUrl', {
      worktreePath,
      sessionId,
      url,
    }),
  tabUpdateTmuxSessionName: (worktreePath: string, oldName: string, newName: string) =>
    ipcRenderer.invoke('tab:command:updateTmuxSessionName', {
      worktreePath,
      oldName,
      newName,
    }),
  tabHandlePtyExit: (
    worktreePath: string,
    sessionId: string,
    exitCode: number,
    tmuxSessionName?: string,
  ) =>
    ipcRenderer.invoke('tab:command:handlePtyExit', {
      worktreePath,
      sessionId,
      exitCode,
      tmuxSessionName,
    }),
  tabKillTmuxPane: (worktreePath: string, tabId: string, paneId: string) =>
    ipcRenderer.invoke('tab:command:killTmuxPane', {
      worktreePath,
      tabId,
      paneId,
    }),
  tabReattachTmuxPane: (
    worktreePath: string,
    tabId: string,
    paneId: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) =>
    ipcRenderer.invoke('tab:command:reattachTmuxPane', {
      worktreePath,
      tabId,
      paneId,
      options,
    }),
  tabToggleFocusedInspector: (worktreePath: string, tabId: string) =>
    ipcRenderer.invoke('tab:command:toggleFocusedInspector', {
      worktreePath,
      tabId,
    }),
  tabRestartPane: (
    worktreePath: string,
    tabId: string,
    paneId: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) =>
    ipcRenderer.invoke('tab:command:restartPane', {
      worktreePath,
      tabId,
      paneId,
      options,
    }),
  tabCloseTab: (worktreePath: string, tabId: string) =>
    ipcRenderer.invoke('tab:command:closeTab', {
      worktreePath,
      tabId,
    }),
  tabPrepareCloseTab: (worktreePath: string, tabId: string) =>
    ipcRenderer.invoke('tab:command:prepareCloseTab', {
      worktreePath,
      tabId,
    }) as Promise<TabClosePreflightResult>,
  tabPrepareCloseAllForWorktree: (
    worktreePath: string,
    options?: { confirmedActiveProcesses?: boolean },
  ) =>
    ipcRenderer.invoke('tab:command:prepareCloseAllForWorktree', {
      worktreePath,
      confirmedActiveProcesses: options?.confirmedActiveProcesses,
    }) as Promise<TabCloseAllPreflightResult>,
  tabGetCloseWarning: (worktreePath: string, target: CloseWarningTarget) =>
    ipcRenderer.invoke('tab:command:getCloseWarning', {
      worktreePath,
      target,
    }) as Promise<CloseWarningResult>,
  tabReopenClosedTab: (
    worktreePath: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) =>
    ipcRenderer.invoke('tab:command:reopenClosedTab', {
      worktreePath,
      options,
    }),
  tabClosePane: (worktreePath: string, tabId: string, paneId: string) =>
    ipcRenderer.invoke('tab:command:closePane', {
      worktreePath,
      tabId,
      paneId,
    }),
  tabCloseAllForWorktree: (worktreePath: string) =>
    ipcRenderer.invoke('tab:command:closeAllForWorktree', {
      worktreePath,
    }),
  tabSetActiveTab: (worktreePath: string, tabId: string) =>
    ipcRenderer.invoke('tab:command:setActiveTab', {
      worktreePath,
      tabId,
    }),
  tabMoveTab: (worktreePath: string, fromIndex: number, toIndex: number) =>
    ipcRenderer.invoke('tab:command:moveTab', {
      worktreePath,
      fromIndex,
      toIndex,
    }),
  tabMoveTabToSplit: (
    worktreePath: string,
    sourceTabId: string,
    targetTabId: string,
    targetPaneId: string,
    direction: 'horizontal' | 'vertical',
    position: 'first' | 'second',
  ) =>
    ipcRenderer.invoke('tab:command:moveTabToSplit', {
      worktreePath,
      sourceTabId,
      targetTabId,
      targetPaneId,
      direction,
      position,
    }),
  tabMovePaneToTarget: (
    worktreePath: string,
    sourceTabId: string,
    sourcePaneId: string,
    targetTabId: string,
    targetPaneId: string,
    direction: 'horizontal' | 'vertical',
    position: 'first' | 'second',
  ) =>
    ipcRenderer.invoke('tab:command:movePaneToTarget', {
      worktreePath,
      sourceTabId,
      sourcePaneId,
      targetTabId,
      targetPaneId,
      direction,
      position,
    }),
  tabDetachPaneToTab: (worktreePath: string, sourceTabId: string, sourcePaneId: string) =>
    ipcRenderer.invoke('tab:command:detachPaneToTab', {
      worktreePath,
      sourceTabId,
      sourcePaneId,
    }),
  tabSpawnPane: (
    toolId: string,
    worktreePath: string,
    options?: {
      initialUrl?: string
      profileId?: string
      workspaceName?: string
      branch?: string
      resumeSessionId?: string
    },
  ) => ipcRenderer.invoke('tab:command:spawnPane', { toolId, worktreePath, options }),
  tabSplitPane: (
    worktreePath: string,
    tabId: string,
    paneId: string,
    direction: 'horizontal' | 'vertical',
  ) => ipcRenderer.invoke('tab:command:splitPane', { worktreePath, tabId, paneId, direction }),
  tabFocusPane: (worktreePath: string, tabId: string, paneId: string) =>
    ipcRenderer.invoke('tab:command:focusPane', { worktreePath, tabId, paneId }),
  tabNavigatePaneFocus: (
    worktreePath: string,
    tabId: string,
    direction: 'left' | 'right' | 'up' | 'down',
  ) => ipcRenderer.invoke('tab:command:navigatePaneFocus', { worktreePath, tabId, direction }),
  tabUpdateSplitRatio: (worktreePath: string, tabId: string, splitId: string, ratio: number) =>
    ipcRenderer.invoke('tab:command:updateSplitRatio', { worktreePath, tabId, splitId, ratio }),
  tabRestoreLayout: (
    worktreePath: string,
    layoutJson: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) =>
    ipcRenderer.invoke('tab:command:restoreLayout', {
      worktreePath,
      layoutJson,
      options,
    }),
  tabResumeSuspendedTab: (
    worktreePath: string,
    tabId: string,
    options?: {
      workspaceName?: string
      branch?: string
    },
  ) =>
    ipcRenderer.invoke('tab:command:resumeSuspendedTab', {
      worktreePath,
      tabId,
      options,
    }),
  tabKillAll: () => ipcRenderer.invoke('tab:command:killAll'),
  tabFocusSession: (sessionId: string) =>
    ipcRenderer.invoke('tab:command:focusSession', { sessionId }),
  tabSaveCurrentLayout: (worktreePath: string) =>
    ipcRenderer.invoke('tab:command:saveCurrentLayout', { worktreePath }),
  agentSendTaskContext: (payload: { text: string; worktreePath?: string; sessionId?: string }) =>
    ipcRenderer.invoke('agent:command:sendTaskContext', payload),
  agentSendReviewContext: (payload: { text: string; worktreePath?: string; sessionId?: string }) =>
    ipcRenderer.invoke('agent:command:sendReviewContext', payload),
  agentSendDrawing: (payload: { worktreePath?: string; sessionId?: string }) =>
    ipcRenderer.invoke('agent:command:sendDrawing', payload),

  // Skills
  listSkills: (opts?: { scope?: string; agent?: string; workspaceId?: string | null }) =>
    ipcRenderer.invoke('skills:list', opts),

  getSkill: (id: string) => ipcRenderer.invoke('skills:get', { id }),

  installSkill: (opts: {
    source: string
    agents?: string[]
    scope?: string
    method?: string
    workspaceId?: string | null
    workspacePath?: string
  }) => ipcRenderer.invoke('skills:install', opts),

  removeSkill: (id: string, workspacePath?: string) =>
    ipcRenderer.invoke('skills:remove', { id, workspacePath }),

  updateSkill: (id: string, workspacePath?: string) =>
    ipcRenderer.invoke('skills:update', { id, workspacePath }),

  toggleSkillAgent: (id: string, agent: string, enabled: boolean, workspacePath?: string) =>
    ipcRenderer.invoke('skills:toggleAgent', { id, agent, enabled, workspacePath }),

  scanSkills: (workspacePath?: string) => ipcRenderer.invoke('skills:scan', { workspacePath }),

  deleteSkillFile: (filePath: string) => ipcRenderer.invoke('skills:deleteFile', { filePath }),

  // Agent session
  updateAgentTitle: (sessionId: string, title: string) =>
    ipcRenderer.invoke('agent:updateTitle', { sessionId, title }),

  // Auto-update
  checkForUpdates: () => ipcRenderer.invoke('app:checkForUpdates'),
  installUpdate: () => ipcRenderer.invoke('app:installUpdate'),
  setUpdateChannel: (channel: string) => ipcRenderer.invoke('app:setUpdateChannel', channel),
  setAutoUpdate: (enabled: boolean) => ipcRenderer.invoke('app:setAutoUpdate', enabled),
  setUpdateCheckFrequency: (frequency: string) =>
    ipcRenderer.invoke('app:setUpdateCheckFrequency', frequency),

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

  // Crash reports
  onCrashReport: (
    callback: (data: {
      timestamp: string
      type: string
      errorMessage: string
      stack?: string
      appVersion: string
      electronVersion: string
      os: string
    }) => void,
  ) => {
    const handler = (_event: IpcRendererEvent, data: Parameters<typeof callback>[0]): void =>
      callback(data)
    ipcRenderer.on('app:crashReport', handler)
    return (): void => {
      ipcRenderer.removeListener('app:crashReport', handler)
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
  setFocusedAgentSession: (ptySessionId: string | null) =>
    ipcRenderer.invoke('app:setFocusedAgentSession', { ptySessionId }),
  focusRendererWebContents: () => ipcRenderer.invoke('app:focusRendererWebContents'),

  // Dialog
  openFolder: (defaultPath?: string) =>
    ipcRenderer.invoke('dialog:openFolder', defaultPath ? { defaultPath } : undefined),
  confirmOpenPath: (path: string) => ipcRenderer.invoke('dialog:confirmOpenPath', { path }),

  // Settings export / import
  exportSettings: () =>
    ipcRenderer.invoke('settings:export') as Promise<{
      path: string
      counts: {
        preferences: number
        profiles: number
        credentials: number
        customTools: number
      }
    } | null>,
  importSettings: () =>
    ipcRenderer.invoke('settings:import') as Promise<{
      counts: {
        preferences: number
        profiles: number
        credentials: number
        customTools: number
      }
    } | null>,

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
  gitPullWithPreferences: (payload: { repoRoot: string }) =>
    ipcRenderer.invoke('git:pullWithPreferences', payload),
  gitCommitWorktree: (payload: { repoRoot: string; message: string; stageAll?: boolean }) =>
    ipcRenderer.invoke('git:commitWorktree', payload),
  gitPushWorktree: (payload: { repoRoot: string }) =>
    ipcRenderer.invoke('git:pushWorktree', payload),
  gitFetchWorktree: (payload: { repoRoot: string }) =>
    ipcRenderer.invoke('git:fetchWorktree', payload),
  gitFetch: (repoRoot: string) => ipcRenderer.invoke('git:fetch', { repoRoot }),
  gitFetchAll: (repoRoot: string) => ipcRenderer.invoke('git:fetchAll', { repoRoot }),
  gitStashWorktree: (payload: { repoRoot: string }) =>
    ipcRenderer.invoke('git:stashWorktree', payload),
  gitStash: (repoRoot: string) => ipcRenderer.invoke('git:stash', { repoRoot }),
  gitStashPopWorktree: (payload: { repoRoot: string }) =>
    ipcRenderer.invoke('git:stashPopWorktree', payload),
  gitStashPop: (repoRoot: string) => ipcRenderer.invoke('git:stashPop', { repoRoot }),
  gitBranches: (repoRoot: string) => ipcRenderer.invoke('git:branches', { repoRoot }),
  gitBranchCreate: (repoRoot: string, name: string, baseBranch: string) =>
    ipcRenderer.invoke('git:branchCreate', { repoRoot, name, baseBranch }),
  gitBranchCreateFromHead: (payload: { repoRoot: string; branch: string }) =>
    ipcRenderer.invoke('git:branchCreateFromHead', payload),
  gitCheckout: (repoRoot: string, branch: string) =>
    ipcRenderer.invoke('git:checkout', { repoRoot, branch }),
  gitBranchDelete: (repoRoot: string, name: string, force: boolean) =>
    ipcRenderer.invoke('git:branchDelete', { repoRoot, name, force }),
  gitBranchPrepareDelete: (payload: { repoRoot: string; branch: string }) =>
    ipcRenderer.invoke('git:branchPrepareDelete', payload),
  gitBranchDeleteWithPreflight: (payload: {
    repoRoot: string
    branch: string
    forceIfUnmerged?: boolean
  }) => ipcRenderer.invoke('git:branchDeleteWithPreflight', payload),
  gitBranchDeleteRemote: (repoRoot: string, remote: string, name: string) =>
    ipcRenderer.invoke('git:branchDeleteRemote', { repoRoot, remote, name }),
  gitPushInfo: (repoRoot: string) => ipcRenderer.invoke('git:pushInfo', { repoRoot }),
  gitPreparePush: (payload: { repoRoot: string }) => ipcRenderer.invoke('git:preparePush', payload),
  gitBranchMerged: (repoRoot: string, branch: string) =>
    ipcRenderer.invoke('git:branchMerged', { repoRoot, branch }),
  gitWorktreeAdd: (repoRoot: string, path: string, branch: string, baseBranch: string) =>
    ipcRenderer.invoke('git:worktreeAdd', { repoRoot, path, branch, baseBranch }),
  gitWorktreeCheckout: (
    repoRoot: string,
    path: string,
    branch: string,
    createLocalTracking: boolean,
  ) =>
    ipcRenderer.invoke('git:worktreeCheckout', {
      repoRoot,
      path,
      branch,
      createLocalTracking,
    }),
  gitWorktreeRemove: (repoRoot: string, path: string, force: boolean) =>
    ipcRenderer.invoke('git:worktreeRemove', { repoRoot, path, force }),
  worktreeCreate: (
    payload:
      | {
          repoRoot: string
          worktreePath: string
          mode: 'new'
          branch: string
          baseBranch: string
        }
      | {
          repoRoot: string
          worktreePath: string
          mode: 'existing'
          branch: string
          createLocalTracking?: boolean
        },
  ) => ipcRenderer.invoke('worktree:create', payload),
  worktreePrepareRemove: (payload: { repoRoot: string; worktreePath: string; branch: string }) =>
    ipcRenderer.invoke('worktree:prepareRemove', payload),
  worktreeGetMergedBranches: (payload: { repoRoot: string; branches: string[] }) =>
    ipcRenderer.invoke('worktree:getMergedBranches', payload),
  worktreeListBranches: (payload: { repoRoot: string }) =>
    ipcRenderer.invoke('worktree:listBranches', payload),
  worktreeRefreshBranches: (payload: { repoRoot: string }) =>
    ipcRenderer.invoke('worktree:refreshBranches', payload),
  worktreeRemoveWithBranch: (payload: {
    repoRoot: string
    worktreePath: string
    branch?: string
    deleteBranch?: boolean
    forceOnFailure?: boolean
  }) => ipcRenderer.invoke('worktree:removeWithBranch', payload),
  gitUnmergedCommits: (repoRoot: string, branch: string) =>
    ipcRenderer.invoke('git:unmergedCommits', { repoRoot, branch }),
  gitStatusPorcelain: (repoRoot: string, worktreePath?: string) =>
    ipcRenderer.invoke('git:statusPorcelain', { repoRoot, worktreePath }),
  changesGetDiff: (payload: { worktreePath: string }) =>
    ipcRenderer.invoke('changes:getDiff', payload),
  changesStageFile: (payload: { worktreePath: string; filePath: string }) =>
    ipcRenderer.invoke('changes:stageFile', payload),
  changesRevertFile: (payload: { worktreePath: string; filePath: string }) =>
    ipcRenderer.invoke('changes:revertFile', payload),
  gitDiff: (repoRoot: string) => ipcRenderer.invoke('git:diff', { repoRoot }),
  gitDiffFile: (repoRoot: string, filePath: string) =>
    ipcRenderer.invoke('git:diffFile', { repoRoot, filePath }),
  gitStageFile: (repoRoot: string, filePath: string) =>
    ipcRenderer.invoke('git:stageFile', { repoRoot, filePath }),
  gitRevertFile: (repoRoot: string, filePath: string) =>
    ipcRenderer.invoke('git:revertFile', { repoRoot, filePath }),
  gitGenerateCommitMessage: (repoRoot: string) =>
    ipcRenderer.invoke('git:generateCommitMessage', { repoRoot }),
  gitCreatePR: (
    repoRoot: string,
    params: { title: string; body: string; baseRefName: string; draft: boolean },
  ) => ipcRenderer.invoke('git:createPR', { repoRoot, ...params }),
  gitGetDefaultBranch: (repoRoot: string) =>
    ipcRenderer.invoke('git:getDefaultBranch', { repoRoot }),

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
  getCredentialDecrypted: (id: string, domain: string, purpose: 'autofill' | 'reveal') =>
    ipcRenderer.invoke('credentials:getDecrypted', { id, domain, purpose }) as Promise<{
      id: string
      username: string
      password: string
    } | null>,
  deleteCredential: (id: string) => ipcRenderer.invoke('credentials:delete', { id }),
  listCredentials: () =>
    ipcRenderer.invoke('credentials:getAll') as Promise<
      Array<{
        id: string
        domain: string
        username: string
        title: string
        createdAt: string
        updatedAt: string
      }>
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

  onBrowserOpenUrl: (callback: (data: { browserId: string; url: string }) => void) => {
    const handler = (_event: IpcRendererEvent, data: { browserId: string; url: string }): void =>
      callback(data)
    ipcRenderer.on('browser:openUrl', handler)
    return (): void => {
      ipcRenderer.removeListener('browser:openUrl', handler)
    }
  },

  // Worktree Setup
  runWorktreeSetup: (workspaceId: string, repoRoot: string, newWorktreePath: string) =>
    ipcRenderer.invoke('worktree:runSetup', { workspaceId, repoRoot, newWorktreePath }),
  abortWorktreeSetup: () => ipcRenderer.send('worktree:abortSetup'),

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

  onGitChanged: (
    callback: (info: GitInfo & { repoRoot: string; changes: GitRefreshFlags }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      info: GitInfo & { repoRoot: string; changes: GitRefreshFlags },
    ): void => callback(info)
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
  onToolsChanged: (callback: (tools: ToolDefinition[]) => void) => {
    const handler = (_event: IpcRendererEvent, tools: ToolDefinition[]): void => callback(tools)
    ipcRenderer.on('tools:changed', handler)
    return (): void => {
      ipcRenderer.removeListener('tools:changed', handler)
    }
  },
  onSkillsChanged: (callback: (skills: CanopySkill[]) => void) => {
    const handler = (_event: IpcRendererEvent, skills: CanopySkill[]): void => callback(skills)
    ipcRenderer.on('skills:changed', handler)
    return (): void => {
      ipcRenderer.removeListener('skills:changed', handler)
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
  /**
   * Subscribe to PTY resize broadcasts. Fires whenever any session's PTY
   * is resized (host desktop fit, split pane resize, etc.) so the remote
   * host controller can relay the new dimensions to any connected peer.
   * Returns an unsubscribe function — call it in a cleanup / onDestroy
   * hook to avoid leaking listeners across session teardown.
   */
  onPtyResized: (callback: (sessionId: string, cols: number, rows: number) => void) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { sessionId: string; cols: number; rows: number },
    ): void => callback(data.sessionId, data.cols, data.rows)
    ipcRenderer.on('pty:resized', handler)
    return (): void => {
      ipcRenderer.removeListener('pty:resized', handler)
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

  onRestoreWindow: (
    callback: (data: {
      paths: string[]
      activeWorktreePath?: string
      removedPaths?: string[]
    }) => void,
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { paths: string[]; activeWorktreePath?: string; removedPaths?: string[] },
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
  quickOpenListFiles: (worktreePath: string, force?: boolean) =>
    ipcRenderer.invoke('quickOpen:listFiles', { worktreePath, force }) as Promise<string[]>,
  quickOpenInvalidateCache: (worktreePath: string) =>
    ipcRenderer.invoke('quickOpen:invalidateCache', { worktreePath }) as Promise<void>,

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
  trackerConfigFetchTaskComments: (
    repoRoot: string | undefined,
    taskKey: string,
    trackerId?: string,
  ) => ipcRenderer.invoke('trackerConfig:fetchTaskComments', { repoRoot, taskKey, trackerId }),
  trackerConfigFetchTaskAttachments: (
    repoRoot: string | undefined,
    taskKey: string,
    trackerId?: string,
  ) => ipcRenderer.invoke('trackerConfig:fetchTaskAttachments', { repoRoot, taskKey, trackerId }),
  trackerConfigDownloadAttachment: (
    repoRoot: string | undefined,
    url: string,
    filename: string,
    trackerId?: string,
  ) =>
    ipcRenderer.invoke('trackerConfig:downloadAttachment', {
      repoRoot,
      url,
      filename,
      trackerId,
    }),
  trackerConfigFindTaskByKey: (repoRoot: string | undefined, taskKey: string, trackerId?: string) =>
    ipcRenderer.invoke('trackerConfig:findTaskByKey', { repoRoot, taskKey, trackerId }),

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
  taskTrackerBuildTaskContext: (payload: {
    connectionId: string
    task: {
      key: string
      summary: string
      description: string
      status: string
      priority: string
      type: string
      url?: string
    }
    repoRoot?: string
    trackerId?: string
  }) => ipcRenderer.invoke('taskTracker:buildTaskContext', payload) as Promise<string>,
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
  taskTrackerPrepareBranchFromTask: (payload: {
    connectionId: string
    task: { key: string; type: string; [k: string]: unknown }
    boardId?: string
    branchType?: string
    repoRoot: string
  }) => ipcRenderer.invoke('taskTracker:prepareBranchFromTask', payload),
  taskTrackerCreateBranchFromTask: (payload: {
    connectionId: string
    task: { key: string; type: string; [k: string]: unknown }
    boardId?: string
    branchType?: string
    repoRoot: string
    baseBranch: string
    stashBeforeCreate?: boolean
  }) => ipcRenderer.invoke('taskTracker:createBranchFromTask', payload),
  taskTrackerCreateWorktreeFromTask: (payload: {
    connectionId: string
    task: { key: string; type: string; [k: string]: unknown }
    boardId?: string
    branchType?: string
    repoRoot: string
    worktreePath: string
    baseBranch: string
  }) => ipcRenderer.invoke('taskTracker:createWorktreeFromTask', payload),
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

  // Remote control (WebRTC pairing via QR)
  remote: {
    start: () => ipcRenderer.invoke('remote:start') as Promise<{ pairingUrl: string }>,
    // Best-effort request from the renderer on app mount — brings the
    // signaling server up in passive listen mode iff the user has opted in
    // and has ≥1 trusted device, so a previously paired phone can reconnect
    // without the user starting a new sidebar pairing flow. Never
    // rejects; failures are silently no-oped on the main side.
    ensureListening: () => ipcRenderer.invoke('remote:ensureListening') as Promise<void>,
    stop: () => ipcRenderer.invoke('remote:stop') as Promise<void>,
    getStatus: () => ipcRenderer.invoke('remote:getStatus') as Promise<RemoteSessionStatus>,
    acceptDevice: (remember: boolean) =>
      ipcRenderer.invoke('remote:acceptDevice', { remember }) as Promise<void>,
    rejectDevice: () => ipcRenderer.invoke('remote:rejectDevice') as Promise<void>,
    sendSignal: (msg: unknown) => ipcRenderer.invoke('remote:sendSignal', msg) as Promise<void>,
    listTrustedDevices: () =>
      ipcRenderer.invoke('remote:listTrustedDevices') as Promise<
        Array<{
          deviceId: string
          name: string
          addedAt: string
          lastSeen: string
          publicKeyJwk: unknown
        }>
      >,
    removeTrustedDevice: (deviceId: string) =>
      ipcRenderer.invoke('remote:removeTrustedDevice', { deviceId }) as Promise<void>,
    listNetworkInterfaces: () =>
      ipcRenderer.invoke('remote:listNetworkInterfaces') as Promise<
        Array<{ name: string; address: string; virtual: boolean }>
      >,
    onStatusChange: (callback: (status: RemoteSessionStatus) => void) => {
      const handler = (_event: IpcRendererEvent, status: RemoteSessionStatus): void =>
        callback(status)
      ipcRenderer.on('remote:statusChange', handler)
      return (): void => {
        ipcRenderer.removeListener('remote:statusChange', handler)
      }
    },
    onSignal: (callback: (msg: unknown) => void) => {
      const handler = (_event: IpcRendererEvent, msg: unknown): void => callback(msg)
      ipcRenderer.on('remote:signal', handler)
      return (): void => {
        ipcRenderer.removeListener('remote:signal', handler)
      }
    },
  },

  // Performance diagnostics (only active when CANOPY_PERF=1)
  ...(process.env.CANOPY_PERF === '1'
    ? {
        perfDiagnostics: () => ipcRenderer.invoke('perf:diagnostics'),
        perfIpcLog: () => ipcRenderer.invoke('perf:ipcLog'),
        perfOpenProject: (path: string) => ipcRenderer.invoke('perf:openProject', { path }),
      }
    : {}),

  // Status-bar perf HUD (always available, opt-in via preference)
  perfHud: {
    start: () => ipcRenderer.invoke('perf:hud:start') as Promise<void>,
    stop: () => ipcRenderer.invoke('perf:hud:stop') as Promise<void>,
    onMetrics: (callback: (metrics: { cpu: number; memMb: number }) => void) => {
      const handler = (_event: IpcRendererEvent, metrics: { cpu: number; memMb: number }): void =>
        callback(metrics)
      ipcRenderer.on('perf:hud:metrics', handler)
      return (): void => {
        ipcRenderer.removeListener('perf:hud:metrics', handler)
      }
    },
  },

  // File utilities
  getPathForFile: (file: File) => webUtils.getPathForFile(file),

  // Platform
  platform: process.platform,

  // Agent Profiles
  listProfiles: (agentType?: AgentType) =>
    ipcRenderer.invoke('profile:list', { agentType }) as Promise<AgentProfileMasked[]>,
  getProfile: (id: string) =>
    ipcRenderer.invoke('profile:get', { id }) as Promise<AgentProfileMasked | null>,
  saveProfile: (input: ProfileInput) =>
    ipcRenderer.invoke('profile:save', input) as Promise<AgentProfileMasked>,
  deleteProfile: (id: string) => ipcRenderer.invoke('profile:delete', { id }) as Promise<void>,
  onProfilesChanged: (callback: (profiles: AgentProfileMasked[]) => void) => {
    const handler = (_event: IpcRendererEvent, data: AgentProfileMasked[]): void => callback(data)
    ipcRenderer.on('profile:changed', handler)
    return () => ipcRenderer.removeListener('profile:changed', handler)
  },

  // Run Configurations
  runConfigDiscover: (repoRoot: string) => ipcRenderer.invoke('runConfig:discover', { repoRoot }),
  runConfigSave: (configDir: string, config: unknown) =>
    ipcRenderer.invoke('runConfig:save', { configDir, config }),
  runConfigAddConfig: (configDir: string, configuration: unknown) =>
    ipcRenderer.invoke('runConfig:addConfig', { configDir, configuration }),
  runConfigUpdateConfig: (configDir: string, name: string, configuration: unknown) =>
    ipcRenderer.invoke('runConfig:updateConfig', { configDir, name, configuration }),
  runConfigDeleteConfig: (configDir: string, name: string) =>
    ipcRenderer.invoke('runConfig:deleteConfig', { configDir, name }),
  runConfigExecuteCommand: (configDir: string, name: string, cwd: string) =>
    ipcRenderer.invoke('runConfig:command:execute', { configDir, name, cwd }),
  runConfigListRunning: () => ipcRenderer.invoke('runConfig:command:listRunning'),
  onRunConfigPostRunResult: (
    callback: (data: { success: boolean; command: string; exitCode?: number }) => void,
  ) => {
    const handler = (_event: IpcRendererEvent, data: Parameters<typeof callback>[0]): void =>
      callback(data)
    ipcRenderer.on('runConfig:postRunResult', handler)
    return () => ipcRenderer.removeListener('runConfig:postRunResult', handler)
  },
}

if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('api', api)
  } catch (error) {
    console.error(error)
  }
} else {
  // SECURITY: refuse to expose the bridge directly on `window` — that would let
  // any page script reach IPC handlers (filesystem, PTY, credentials) without
  // isolation. Context isolation is required.
  throw new Error('Canopy preload requires contextIsolation:true')
}
