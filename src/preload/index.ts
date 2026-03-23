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
  spawnTool: (toolId: string, worktreePath: string, options?: { cols?: number; rows?: number }) =>
    ipcRenderer.invoke('tool:spawn', { toolId, worktreePath, ...options }),

  // Dialog
  openFolder: () => ipcRenderer.invoke('dialog:openFolder'),

  // Git
  gitDetect: (path: string) => ipcRenderer.invoke('git:detect', { path }),
  gitWorktrees: (repoRoot: string) => ipcRenderer.invoke('git:worktrees', { repoRoot }),
  gitStatus: (repoRoot: string) => ipcRenderer.invoke('git:status', { repoRoot }),
  gitWatch: (repoRoot: string) => ipcRenderer.invoke('git:watch', { repoRoot }),
  gitUnwatch: () => ipcRenderer.invoke('git:unwatch'),

  // Push events (main → renderer)
  onGitChanged: (callback: (info: unknown) => void) => {
    const handler = (_event: IpcRendererEvent, info: unknown): void => callback(info)
    ipcRenderer.on('git:changed', handler)
    return (): void => {
      ipcRenderer.removeListener('git:changed', handler)
    }
  },
  onPtyExit: (
    callback: (data: { sessionId: string; exitCode: number; signal: number }) => void
  ) => {
    const handler = (
      _event: IpcRendererEvent,
      data: { sessionId: string; exitCode: number; signal: number }
    ): void => callback(data)
    ipcRenderer.on('pty:exit', handler)
    return (): void => {
      ipcRenderer.removeListener('pty:exit', handler)
    }
  }
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
