import { ipcMain, dialog } from 'electron'
import type { BrowserWindow } from 'electron'
import type { PtyManager } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import { GitRepository } from '../git/GitRepository'
import { GitWatcher } from '../git/GitWatcher'

let activeWatcher: GitWatcher | null = null
let activeWorkspaceId: string | null = null

export function disposeGitWatcher(): void {
  if (activeWatcher) {
    activeWatcher.stop()
    activeWatcher = null
  }
  activeWorkspaceId = null
}

export function registerIpcHandlers(
  ptyManager: PtyManager,
  wsBridge: WsBridge,
  workspaceStore: WorkspaceStore,
  preferencesStore: PreferencesStore,
  toolRegistry: ToolRegistry,
  mainWindow: BrowserWindow
): void {
  // --- PTY ---

  ipcMain.handle(
    'pty:spawn',
    async (_event, options?: { cols?: number; rows?: number; cwd?: string }) => {
      const session = ptyManager.spawn(options)
      const wsUrl = await wsBridge.create(session.id, session.pty)
      return { sessionId: session.id, wsUrl }
    }
  )

  ipcMain.handle(
    'pty:resize',
    (_event, payload: { sessionId: string; cols: number; rows: number }) => {
      ptyManager.resize(payload.sessionId, payload.cols, payload.rows)
    }
  )

  ipcMain.handle('pty:kill', (_event, payload: { sessionId: string }) => {
    wsBridge.destroy(payload.sessionId)
    ptyManager.kill(payload.sessionId)
  })

  // --- Workspaces ---

  ipcMain.handle('db:workspace:list', (_event, payload?: { limit?: number }) => {
    return workspaceStore.list(payload?.limit)
  })

  ipcMain.handle('db:workspace:get', (_event, payload: { id: string }) => {
    return workspaceStore.get(payload.id) ?? null
  })

  ipcMain.handle('db:workspace:getByPath', (_event, payload: { path: string }) => {
    return workspaceStore.getByPath(payload.path) ?? null
  })

  ipcMain.handle(
    'db:workspace:upsert',
    (_event, payload: { path: string; name: string; isGitRepo: boolean }) => {
      return workspaceStore.upsert(payload)
    }
  )

  ipcMain.handle('db:workspace:remove', (_event, payload: { id: string }) => {
    workspaceStore.remove(payload.id)
  })

  ipcMain.handle('db:workspace:touch', (_event, payload: { id: string }) => {
    workspaceStore.touch(payload.id)
  })

  // --- Preferences ---

  ipcMain.handle('db:prefs:get', (_event, payload: { key: string }) => {
    return preferencesStore.get(payload.key)
  })

  ipcMain.handle('db:prefs:set', (_event, payload: { key: string; value: string }) => {
    preferencesStore.set(payload.key, payload.value)
  })

  ipcMain.handle('db:prefs:getAll', () => {
    return preferencesStore.getAll()
  })

  ipcMain.handle('db:prefs:delete', (_event, payload: { key: string }) => {
    preferencesStore.delete(payload.key)
  })

  // --- Tools ---

  ipcMain.handle('tools:list', () => {
    return toolRegistry.getAll()
  })

  ipcMain.handle('tools:get', (_event, payload: { id: string }) => {
    return toolRegistry.get(payload.id) ?? null
  })

  ipcMain.handle('tools:checkAvailability', async () => {
    return toolRegistry.checkAvailability()
  })

  // --- Dialog ---

  ipcMain.handle('dialog:openFolder', async () => {
    const result = await dialog.showOpenDialog(mainWindow, {
      properties: ['openDirectory', 'createDirectory']
    })
    return result.canceled ? null : result.filePaths[0]
  })

  // --- Git ---

  ipcMain.handle('git:detect', async (_event, payload: { path: string }) => {
    return GitRepository.detect(payload.path)
  })

  ipcMain.handle('git:worktrees', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.listWorktrees(payload.repoRoot)
  })

  ipcMain.handle('git:status', async (_event, payload: { repoRoot: string }) => {
    const [branch, isDirty, aheadBehind] = await Promise.all([
      GitRepository.getBranch(payload.repoRoot),
      GitRepository.isDirty(payload.repoRoot),
      GitRepository.getAheadBehind(payload.repoRoot)
    ])
    return { branch, isDirty, aheadBehind }
  })

  ipcMain.handle('git:watch', async (_event, payload: { repoRoot: string }) => {
    disposeGitWatcher()

    // Find workspace ID for cache updates
    const ws = workspaceStore.getByPath(payload.repoRoot)
    activeWorkspaceId = ws?.id ?? null

    activeWatcher = new GitWatcher(payload.repoRoot, (info) => {
      // Update DB cache
      if (activeWorkspaceId) {
        workspaceStore.updateGitCache(activeWorkspaceId, {
          branch: info.branch,
          dirty: info.isDirty,
          aheadBehind: info.aheadBehind
            ? `${info.aheadBehind.ahead}/${info.aheadBehind.behind}`
            : null,
          worktreeCount: info.worktrees.length
        })
      }
      // Push to renderer
      mainWindow.webContents.send('git:changed', info)
    })
    activeWatcher.start()
  })

  ipcMain.handle('git:unwatch', () => {
    disposeGitWatcher()
  })
}
