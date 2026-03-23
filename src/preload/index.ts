import { contextBridge, ipcRenderer } from 'electron'
import type { IpcRendererEvent } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'

const api = {
  // PTY
  spawnPty: (options?: { cols?: number; rows?: number; cwd?: string }) =>
    ipcRenderer.invoke('pty:spawn', options),
  resizePty: (sessionId: string, cols: number, rows: number) =>
    ipcRenderer.invoke('pty:resize', { sessionId, cols, rows }),
  killPty: (sessionId: string) => ipcRenderer.invoke('pty:kill', { sessionId }),

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

  // Tools
  listTools: () => ipcRenderer.invoke('tools:list'),
  getTool: (id: string) => ipcRenderer.invoke('tools:get', { id }),
  checkToolAvailability: () => ipcRenderer.invoke('tools:checkAvailability'),
  spawnTool: (
    toolId: string,
    worktreePath: string,
    options?: { cols?: number; rows?: number; workspaceName?: string; branch?: string },
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

  // App / Shell
  getHomedir: () => ipcRenderer.invoke('app:homedir') as Promise<string>,
  showInFolder: (path: string) => ipcRenderer.invoke('app:showInFolder', { path }),

  // Dialog
  openFolder: () => ipcRenderer.invoke('dialog:openFolder'),

  // Git
  refreshWorkspaceGitStatus: (id: string, path: string) =>
    ipcRenderer.invoke('db:workspace:refreshGitStatus', { id, path }),

  gitDetect: (path: string) => ipcRenderer.invoke('git:detect', { path }),
  gitWorktrees: (repoRoot: string) => ipcRenderer.invoke('git:worktrees', { repoRoot }),
  gitStatus: (repoRoot: string) => ipcRenderer.invoke('git:status', { repoRoot }),
  gitWatch: (repoRoot: string) => ipcRenderer.invoke('git:watch', { repoRoot }),
  gitUnwatch: () => ipcRenderer.invoke('git:unwatch'),

  // Git Operations
  gitCommit: (repoRoot: string, message: string) =>
    ipcRenderer.invoke('git:commit', { repoRoot, message }),
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

  // Layouts
  saveLayout: (workspaceId: string, worktreePath: string, layoutJson: string) =>
    ipcRenderer.invoke('layout:save', { workspaceId, worktreePath, layoutJson }),
  getLayout: (workspaceId: string, worktreePath: string) =>
    ipcRenderer.invoke('layout:get', { workspaceId, worktreePath }),
  getAllLayouts: (workspaceId: string) => ipcRenderer.invoke('layout:getAll', { workspaceId }),

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
}

if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('electron', electronAPI)
    contextBridge.exposeInMainWorld('api', api)
  } catch (error) {
    console.error(error)
  }
} else {
  // @ts-ignore (define in dts)
  window.electron = electronAPI
  // @ts-ignore (define in dts)
  window.api = api
}
