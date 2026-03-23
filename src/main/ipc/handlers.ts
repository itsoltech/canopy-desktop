import { ipcMain, dialog, shell, BrowserWindow } from 'electron'
import os from 'os'
import type { PtyManager } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { LayoutStore } from '../db/LayoutStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import type { ClaudeSessionManager } from '../claude/ClaudeSessionManager'
import { GitRepository } from '../git/GitRepository'
import { GitWatcher } from '../git/GitWatcher'

function resolveShellArgs(): string[] {
  if (os.platform() === 'win32') return []
  return ['--login']
}

let activeWatcher: GitWatcher | null = null
let activeWorkspaceId: string | null = null

export function disposeGitWatcher(): void {
  if (activeWatcher) {
    activeWatcher.stop()
    activeWatcher = null
  }
  activeWorkspaceId = null
}

function safeSend(
  getMainWindow: () => BrowserWindow | null,
  channel: string,
  ...args: unknown[]
): void {
  const win = getMainWindow()
  if (win && !win.isDestroyed()) {
    win.webContents.send(channel, ...args)
  }
}

export function registerIpcHandlers(
  ptyManager: PtyManager,
  wsBridge: WsBridge,
  workspaceStore: WorkspaceStore,
  preferencesStore: PreferencesStore,
  layoutStore: LayoutStore,
  toolRegistry: ToolRegistry,
  claudeSessionManager: ClaudeSessionManager,
  getMainWindow: () => BrowserWindow | null,
): void {
  // --- PTY ---

  ipcMain.handle(
    'pty:spawn',
    async (_event, options?: { cols?: number; rows?: number; cwd?: string }) => {
      const session = ptyManager.spawn(options)
      const wsUrl = await wsBridge.create(session.id, session.pty)

      session.pty.onExit(({ exitCode, signal }) => {
        safeSend(getMainWindow, 'pty:exit', { sessionId: session.id, exitCode, signal })
      })

      return { sessionId: session.id, wsUrl }
    },
  )

  ipcMain.handle(
    'pty:resize',
    (_event, payload: { sessionId: string; cols: number; rows: number }) => {
      ptyManager.resize(payload.sessionId, payload.cols, payload.rows)
    },
  )

  ipcMain.handle('pty:kill', (_event, payload: { sessionId: string }) => {
    wsBridge.destroy(payload.sessionId)
    ptyManager.kill(payload.sessionId)
  })

  // --- Tool Spawning ---

  ipcMain.handle(
    'tool:spawn',
    async (
      event,
      payload: {
        toolId: string
        worktreePath: string
        cols?: number
        rows?: number
        workspaceName?: string
        branch?: string
      },
    ) => {
      const tool = toolRegistry.get(payload.toolId)
      if (!tool) throw new Error(`Unknown tool: ${payload.toolId}`)

      const command = toolRegistry.resolveCommand(tool)
      const isShell = tool.id === 'shell' || tool.command === 'shell'
      const isClaude = tool.id === 'claude'
      let args = isShell ? resolveShellArgs() : [...tool.args]
      let env: Record<string, string> | undefined

      let claudeTempId: string | undefined
      if (isClaude) {
        const senderWindow = BrowserWindow.fromWebContents(event.sender)
        if (!senderWindow) throw new Error('No window for Claude session')
        const claudeSession = await claudeSessionManager.createSession(
          payload.worktreePath,
          payload.workspaceName ?? '',
          payload.branch ?? null,
          senderWindow,
        )
        args = ['--settings', claudeSession.settingsPath, ...args]
        env = { NIXTTY_HOOK_PORT: String(claudeSession.hookPort) }
        claudeTempId = claudeSession.tempId
      }

      const session = ptyManager.spawn({
        command,
        args,
        cwd: payload.worktreePath,
        cols: payload.cols,
        rows: payload.rows,
        env,
      })

      if (isClaude && claudeTempId) {
        claudeSessionManager.rekey(claudeTempId, session.id)
      }

      const wsUrl = await wsBridge.create(session.id, session.pty)

      session.pty.onExit(({ exitCode, signal }) => {
        safeSend(getMainWindow, 'pty:exit', { sessionId: session.id, exitCode, signal })
        if (isClaude) {
          claudeSessionManager.destroySession(session.id)
        }
      })

      return { sessionId: session.id, wsUrl, toolId: tool.id, toolName: tool.name }
    },
  )

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
    },
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

  // --- App / Shell ---

  ipcMain.handle('app:showInFolder', (_event, payload: { path: string }) => {
    shell.showItemInFolder(payload.path)
  })

  // --- Dialog ---

  ipcMain.handle('dialog:openFolder', async () => {
    const win = getMainWindow()
    if (!win || win.isDestroyed()) return null
    const result = await dialog.showOpenDialog(win, {
      properties: ['openDirectory', 'createDirectory'],
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
      GitRepository.getAheadBehind(payload.repoRoot),
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
          worktreeCount: info.worktrees.length,
        })
      }
      // Push to renderer
      safeSend(getMainWindow, 'git:changed', info)
    })
    activeWatcher.start()
  })

  ipcMain.handle('git:unwatch', () => {
    disposeGitWatcher()
  })

  // --- Workspace Git Status Refresh ---

  ipcMain.handle(
    'db:workspace:refreshGitStatus',
    async (_event, payload: { id: string; path: string }) => {
      const info = await GitRepository.detect(payload.path)
      const aheadBehind = info.aheadBehind ? JSON.stringify(info.aheadBehind) : null
      workspaceStore.updateGitCache(payload.id, {
        branch: info.branch,
        dirty: info.isDirty,
        aheadBehind,
        worktreeCount: info.worktrees.length,
      })
      return workspaceStore.get(payload.id) ?? null
    },
  )

  // --- Git Operations ---

  ipcMain.handle('git:commit', async (_event, payload: { repoRoot: string; message: string }) => {
    return GitRepository.commit(payload.repoRoot, payload.message)
  })

  ipcMain.handle('git:push', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.push(payload.repoRoot)
  })

  ipcMain.handle('git:pull', async (_event, payload: { repoRoot: string; rebase: boolean }) => {
    return GitRepository.pull(payload.repoRoot, payload.rebase)
  })

  ipcMain.handle('git:fetch', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.fetch(payload.repoRoot)
  })

  ipcMain.handle('git:fetchAll', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.fetchAll(payload.repoRoot)
  })

  ipcMain.handle('git:stash', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.stash(payload.repoRoot)
  })

  ipcMain.handle('git:stashPop', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.stashPop(payload.repoRoot)
  })

  ipcMain.handle('git:branches', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.listBranches(payload.repoRoot)
  })

  ipcMain.handle(
    'git:branchCreate',
    async (_event, payload: { repoRoot: string; name: string; baseBranch: string }) => {
      return GitRepository.createBranch(payload.repoRoot, payload.name, payload.baseBranch)
    },
  )

  ipcMain.handle(
    'git:branchDelete',
    async (_event, payload: { repoRoot: string; name: string; force: boolean }) => {
      return GitRepository.deleteBranch(payload.repoRoot, payload.name, payload.force)
    },
  )

  ipcMain.handle(
    'git:branchDeleteRemote',
    async (_event, payload: { repoRoot: string; remote: string; name: string }) => {
      return GitRepository.deleteRemoteBranch(payload.repoRoot, payload.remote, payload.name)
    },
  )

  ipcMain.handle('git:pushInfo', async (_event, payload: { repoRoot: string }) => {
    return GitRepository.getPushInfo(payload.repoRoot)
  })

  ipcMain.handle(
    'git:branchMerged',
    async (_event, payload: { repoRoot: string; branch: string }) => {
      return GitRepository.isBranchMerged(payload.repoRoot, payload.branch)
    },
  )

  ipcMain.handle(
    'git:worktreeAdd',
    async (
      _event,
      payload: { repoRoot: string; path: string; branch: string; baseBranch: string },
    ) => {
      return GitRepository.worktreeAdd(
        payload.repoRoot,
        payload.path,
        payload.branch,
        payload.baseBranch,
      )
    },
  )

  ipcMain.handle(
    'git:worktreeRemove',
    async (_event, payload: { repoRoot: string; path: string; force: boolean }) => {
      return GitRepository.worktreeRemove(payload.repoRoot, payload.path, payload.force)
    },
  )

  ipcMain.handle(
    'git:unmergedCommits',
    async (_event, payload: { repoRoot: string; branch: string }) => {
      return GitRepository.getUnmergedCommits(payload.repoRoot, payload.branch)
    },
  )

  ipcMain.handle(
    'git:statusPorcelain',
    async (_event, payload: { repoRoot: string; worktreePath?: string }) => {
      return GitRepository.getStatusPorcelain(payload.repoRoot, payload.worktreePath)
    },
  )

  // --- Layouts ---

  ipcMain.handle(
    'layout:save',
    (_event, payload: { workspaceId: string; worktreePath: string; layoutJson: string }) => {
      try {
        layoutStore.save(payload.workspaceId, payload.worktreePath, payload.layoutJson)
      } catch {
        // DB may already be closed during shutdown
      }
    },
  )

  ipcMain.handle('layout:get', (_event, payload: { workspaceId: string; worktreePath: string }) => {
    return layoutStore.get(payload.workspaceId, payload.worktreePath)
  })

  ipcMain.handle('layout:getAll', (_event, payload: { workspaceId: string }) => {
    return layoutStore.getAll(payload.workspaceId)
  })

  // --- Custom Tools ---

  ipcMain.handle(
    'tools:addCustom',
    (
      _event,
      payload: {
        id: string
        name: string
        command: string
        args?: string[]
        icon?: string
        category?: string
      },
    ) => {
      toolRegistry.addCustom(payload)
      return toolRegistry.getAll()
    },
  )

  ipcMain.handle('tools:removeCustom', (_event, payload: { id: string }) => {
    toolRegistry.removeCustom(payload.id)
    return toolRegistry.getAll()
  })
}
