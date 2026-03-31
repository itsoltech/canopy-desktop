import { ipcMain, dialog, shell, BrowserWindow } from 'electron'
import os from 'os'
import fs from 'fs'
import path from 'path'
import type { PtyManager } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { LayoutStore } from '../db/LayoutStore'
import type { OnboardingStore } from '../db/OnboardingStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import type { AgentSessionManager } from '../agents/AgentSessionManager'
import type { WindowManager } from '../WindowManager'
import type { BrowserManager } from '../browser/BrowserManager'
import { execFile } from 'child_process'
import { promisify } from 'util'
import { GitRepository } from '../git/GitRepository'
import { GitWatcher } from '../git/GitWatcher'
import { runWorktreeSetup } from '../worktree/WorktreeSetupRunner'

const execFileAsync = promisify(execFile)
import type { WorktreeSetupAction } from '../db/types'
import { generateCommitMessage } from '../ai/commitMessageGenerator'

function resolveShellArgs(): string[] {
  if (os.platform() === 'win32') return []
  return ['--login']
}

export function registerIpcHandlers(
  ptyManager: PtyManager,
  wsBridge: WsBridge,
  workspaceStore: WorkspaceStore,
  preferencesStore: PreferencesStore,
  layoutStore: LayoutStore,
  toolRegistry: ToolRegistry,
  agentSessionManager: AgentSessionManager,
  windowManager: WindowManager,
  browserManager: BrowserManager,
  onboardingStore: OnboardingStore,
): void {
  // --- PTY ---

  ipcMain.handle(
    'pty:spawn',
    async (event, options?: { cols?: number; rows?: number; cwd?: string }) => {
      const sender = event.sender
      const session = ptyManager.spawn(options)
      const wsUrl = await wsBridge.create(session.id, session.pty)

      windowManager.trackPtySession(sender.id, session.id)

      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', { sessionId: session.id, exitCode, signal })
        }
        windowManager.untrackPtySession(sender.id, session.id)
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

  ipcMain.handle('pty:write', (_event, payload: { sessionId: string; data: string }) => {
    ptyManager.write(payload.sessionId, payload.data)
  })

  ipcMain.handle('pty:hasChildProcess', (_event, payload: { sessionId: string }) => {
    return ptyManager.hasChildProcess(payload.sessionId)
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
        resumeSessionId?: string
      },
    ) => {
      const sender = event.sender
      const tool = toolRegistry.get(payload.toolId)
      if (!tool) throw new Error(`Unknown tool: ${payload.toolId}`)

      const command = toolRegistry.resolveCommand(tool)
      const isShell = tool.id === 'shell' || tool.command === 'shell'
      const isAgent = agentSessionManager.isAgentTool(tool.id)
      let args = isShell ? resolveShellArgs() : [...tool.args]
      let env: Record<string, string> | undefined

      let agentTempId: string | undefined
      if (isAgent) {
        const senderWindow = BrowserWindow.fromWebContents(sender)
        if (!senderWindow) throw new Error('No window for agent session')

        // Parse settings.json overrides from prefs
        let settingsOverrides: Record<string, unknown> | undefined
        const settingsJsonRaw = preferencesStore.get(`${tool.id}.settingsJson`)
        if (settingsJsonRaw) {
          try {
            settingsOverrides = JSON.parse(settingsJsonRaw) as Record<string, unknown>
          } catch {
            // Invalid JSON
          }
        }

        const agentSession = await agentSessionManager.createSession(
          tool.id,
          payload.worktreePath,
          payload.workspaceName ?? '',
          payload.branch ?? null,
          senderWindow,
          settingsOverrides,
        )
        args = [...agentSession.settingsArgs, ...args]
        if (payload.resumeSessionId) {
          args.push(...agentSessionManager.getResumeArgs(tool.id, payload.resumeSessionId))
        }
        args.push(...agentSessionManager.getCliArgs(tool.id, preferencesStore))
        env = {
          CANOPY_HOOK_PORT: String(agentSession.hookPort),
          CANOPY_HOOK_TOKEN: agentSession.hookAuthToken,
          ...agentSession.settingsEnv,
          ...agentSessionManager.getEnvVars(tool.id, preferencesStore),
        }
        agentTempId = agentSession.tempId
      }

      const session = ptyManager.spawn({
        command,
        args,
        cwd: payload.worktreePath,
        cols: payload.cols,
        rows: payload.rows,
        env,
      })

      if (isAgent && agentTempId) {
        agentSessionManager.rekey(agentTempId, session.id)
      }

      const wsUrl = await wsBridge.create(session.id, session.pty)

      windowManager.trackPtySession(sender.id, session.id)

      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', { sessionId: session.id, exitCode, signal })
        }
        windowManager.untrackPtySession(sender.id, session.id)
        if (isAgent) {
          agentSessionManager.destroySession(session.id)
        }
      })

      return { sessionId: session.id, wsUrl, toolId: tool.id, toolName: tool.name }
    },
  )

  ipcMain.handle('agent:updateTitle', (_event, payload: { sessionId: string; title: string }) => {
    agentSessionManager.updateProcessTitle(payload.sessionId, payload.title)
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

  ipcMain.handle('app:homedir', () => os.homedir())

  ipcMain.handle('app:showInFolder', (_event, payload: { path: string }) => {
    shell.showItemInFolder(payload.path)
  })

  // --- App: Multi-window ---

  ipcMain.handle('app:newWindow', () => {
    windowManager.createWindow()
  })

  ipcMain.handle('app:setWorkspacePath', (event, payload: { path: string }) => {
    windowManager.addWorkspacePath(event.sender.id, payload.path)
    const configs = windowManager.getAllWindowConfigs()
    if (configs.length > 0) {
      preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
    } else {
      preferencesStore.delete('openWindowConfigs')
    }
  })

  ipcMain.handle('app:setActiveWorktree', (event, payload: { path: string }) => {
    windowManager.setActiveWorktree(event.sender.id, payload.path)
  })

  ipcMain.handle(
    'app:setFocusedAgentSession',
    (event, payload: { ptySessionId: string | null }) => {
      windowManager.setFocusedAgentSession(event.sender.id, payload.ptySessionId)
    },
  )

  ipcMain.handle('app:detachProject', (event, payload: { path: string }) => {
    const senderId = event.sender.id
    windowManager.removeWorkspacePath(senderId, payload.path)
    windowManager.disposeGitWatcher(senderId, payload.path)
    const configs = windowManager.getAllWindowConfigs()
    if (configs.length > 0) {
      preferencesStore.set('openWindowConfigs', JSON.stringify(configs))
    } else {
      preferencesStore.delete('openWindowConfigs')
    }
  })

  ipcMain.handle('app:focusWindowForPath', (event, payload: { path: string }) => {
    const existing = windowManager.getWindowForPath(payload.path)
    if (existing && existing.webContents.id !== event.sender.id) {
      if (existing.isMinimized()) existing.restore()
      existing.focus()
      return true
    }
    return false
  })

  ipcMain.handle('app:focusRendererWebContents', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (win && !win.isDestroyed()) {
      win.webContents.focus()
    }
  })

  // --- Dialog ---

  ipcMain.handle('dialog:openFolder', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
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

  ipcMain.handle('git:status', async (_event, payload: { path: string }) => {
    const [branch, isDirty, aheadBehind] = await Promise.all([
      GitRepository.getBranch(payload.path),
      GitRepository.isDirty(payload.path),
      GitRepository.getAheadBehind(payload.path),
    ])
    return { branch, isDirty, aheadBehind }
  })

  ipcMain.handle('git:watch', async (event, payload: { repoRoot: string }) => {
    const senderId = event.sender.id

    // Dispose previous watcher for this specific repo only
    windowManager.disposeGitWatcher(senderId, payload.repoRoot)

    // Find workspace ID for cache updates
    const ws = workspaceStore.getByPath(payload.repoRoot)
    const workspaceId = ws?.id ?? null

    const watcher = new GitWatcher(payload.repoRoot, (info) => {
      if (workspaceId) {
        workspaceStore.updateGitCache(workspaceId, {
          branch: info.branch,
          dirty: info.isDirty,
          aheadBehind: info.aheadBehind
            ? `${info.aheadBehind.ahead}/${info.aheadBehind.behind}`
            : null,
          worktreeCount: info.worktrees.length,
        })
      }
      if (!event.sender.isDestroyed()) {
        event.sender.send('git:changed', { ...info, repoRoot: payload.repoRoot })
      }
    })
    watcher.start()
    windowManager.setGitWatcher(senderId, payload.repoRoot, watcher)
  })

  ipcMain.handle('git:unwatch', (event, payload?: { repoRoot?: string }) => {
    if (payload?.repoRoot) {
      windowManager.disposeGitWatcher(event.sender.id, payload.repoRoot)
    } else {
      windowManager.disposeAllGitWatchers(event.sender.id)
    }
  })

  ipcMain.handle('git:init', async (_event, payload: { path: string }) => {
    await execFileAsync('git', ['init'], { cwd: payload.path })
    return GitRepository.detect(payload.path)
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

  ipcMain.handle(
    'git:commit',
    async (_event, payload: { repoRoot: string; message: string; stageAll?: boolean }) => {
      return GitRepository.commit(payload.repoRoot, payload.message, payload.stageAll)
    },
  )

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
      const resolvedPath = payload.path.startsWith('~/')
        ? os.homedir() + payload.path.slice(1)
        : payload.path
      return GitRepository.worktreeAdd(
        payload.repoRoot,
        resolvedPath,
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

  ipcMain.handle('git:generateCommitMessage', async (_event, payload: { repoRoot: string }) => {
    const diff = await GitRepository.getDiff(payload.repoRoot)
    if (!diff.trim()) return null
    return generateCommitMessage(diff, preferencesStore)
  })

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

  ipcMain.handle(
    'layout:delete',
    (_event, payload: { workspaceId: string; worktreePath: string }) => {
      layoutStore.delete(payload.workspaceId, payload.worktreePath)
    },
  )

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

  // --- Browser ---

  ipcMain.handle('browser:create', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (!win) throw new Error('No window for browser view')
    const browserId = crypto.randomUUID()
    browserManager.create(browserId, win, event.sender)
    return { browserId }
  })

  ipcMain.handle('browser:destroy', (_event, payload: { browserId: string }) => {
    browserManager.destroy(payload.browserId)
  })

  ipcMain.handle('browser:navigate', (_event, payload: { browserId: string; url: string }) => {
    browserManager.navigate(payload.browserId, payload.url)
  })

  ipcMain.handle('browser:back', (_event, payload: { browserId: string }) => {
    browserManager.goBack(payload.browserId)
  })

  ipcMain.handle('browser:forward', (_event, payload: { browserId: string }) => {
    browserManager.goForward(payload.browserId)
  })

  ipcMain.handle('browser:reload', (_event, payload: { browserId: string }) => {
    browserManager.reload(payload.browserId)
  })

  ipcMain.handle(
    'browser:setBounds',
    (
      _event,
      payload: {
        browserId: string
        bounds: { x: number; y: number; width: number; height: number }
      },
    ) => {
      browserManager.setBounds(payload.browserId, payload.bounds)
    },
  )

  ipcMain.handle(
    'browser:setVisible',
    (_event, payload: { browserId: string; visible: boolean }) => {
      browserManager.setVisible(payload.browserId, payload.visible)
    },
  )

  ipcMain.handle(
    'browser:toggleDevTools',
    (_event, payload: { browserId: string; mode?: 'bottom' | 'right' }) => {
      browserManager.toggleDevTools(payload.browserId, payload.mode)
    },
  )

  ipcMain.handle('browser:getState', (_event, payload: { browserId: string }) => {
    return browserManager.getState(payload.browserId)
  })

  ipcMain.handle('browser:capturePageFull', async (_event, payload: { browserId: string }) => {
    return browserManager.capturePageFull(payload.browserId)
  })

  ipcMain.handle('browser:startElementPick', async (_event, payload: { browserId: string }) => {
    return browserManager.startElementPick(payload.browserId)
  })

  ipcMain.handle('browser:startRegionCapture', async (_event, payload: { browserId: string }) => {
    return browserManager.startRegionCapture(payload.browserId)
  })

  ipcMain.handle('browser:cancelPick', (_event, payload: { browserId: string }) => {
    browserManager.cancelPick(payload.browserId)
  })

  // --- Filesystem ---

  const IGNORED_NAMES = new Set([
    '.git',
    'node_modules',
    '.next',
    '__pycache__',
    '.DS_Store',
    '.svelte-kit',
    '.turbo',
    '.nuxt',
    '.output',
  ])

  async function validatePathAccess(wcId: number, targetPath: string): Promise<void> {
    const resolved = await fs.promises.realpath(targetPath)
    const allowed = windowManager.getWorkspacePaths(wcId)
    const ok = allowed.some((wp) => resolved === wp || resolved.startsWith(wp + path.sep))
    if (!ok) throw new Error('Access denied: path outside workspace')
  }

  ipcMain.handle('fs:readDir', async (event, payload: { dirPath: string }) => {
    await validatePathAccess(event.sender.id, payload.dirPath)
    const entries = await fs.promises.readdir(payload.dirPath, { withFileTypes: true })
    const filtered = entries.filter((e) => {
      if (IGNORED_NAMES.has(e.name)) return false
      if (e.name.startsWith('.') && e.name !== '.env.example') return false
      return true
    })
    const results = await Promise.all(
      filtered.map(async (entry) => {
        const isDir = entry.isDirectory()
        let size = 0
        if (!isDir) {
          try {
            const s = await fs.promises.stat(path.join(payload.dirPath, entry.name))
            size = s.size
          } catch {
            return null
          }
        }
        return { name: entry.name, isDirectory: isDir, size }
      }),
    )
    return results
      .filter((r): r is { name: string; isDirectory: boolean; size: number } => r !== null)
      .sort((a, b) => {
        if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
        return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
      })
  })

  ipcMain.handle('fs:readFile', async (event, payload: { filePath: string; maxBytes?: number }) => {
    await validatePathAccess(event.sender.id, payload.filePath)
    const maxBytes = Math.min(payload.maxBytes ?? 1_048_576, 10_485_760)
    const stat = await fs.promises.stat(payload.filePath)
    const size = stat.size

    const fd = await fs.promises.open(payload.filePath, 'r')
    try {
      const readSize = Math.min(size, maxBytes)
      const buf = Buffer.alloc(readSize)
      await fd.read(buf, 0, readSize, 0)

      // Binary detection: check first 8KB for null bytes
      const detectEnd = Math.min(readSize, 8192)
      for (let i = 0; i < detectEnd; i++) {
        if (buf[i] === 0) return { binary: true, size }
      }

      return {
        content: buf.toString('utf-8'),
        truncated: size > maxBytes,
        size,
        binary: false,
      }
    } finally {
      await fd.close()
    }
  })

  // --- Worktree Setup ---

  const setupAbortControllers = new Map<number, AbortController>()

  ipcMain.handle(
    'worktree:runSetup',
    async (event, payload: { workspaceId: string; repoRoot: string; newWorktreePath: string }) => {
      const configJson = preferencesStore.get(`workspace:${payload.workspaceId}:worktreeSetup`)
      if (!configJson) return { success: true, errors: [] }

      let actions: WorktreeSetupAction[]
      try {
        actions = JSON.parse(configJson) as WorktreeSetupAction[]
      } catch {
        return { success: false, errors: ['Invalid worktree setup config'] }
      }

      if (actions.length === 0) return { success: true, errors: [] }

      const worktrees = await GitRepository.listWorktrees(payload.repoRoot)
      const mainWorktree = worktrees.find((wt) => wt.isMain)
      const mainWorktreePath = mainWorktree?.path ?? payload.repoRoot

      const sender = event.sender
      const controller = new AbortController()
      setupAbortControllers.set(sender.id, controller)

      try {
        return await runWorktreeSetup(
          actions,
          {
            repoRoot: payload.repoRoot,
            mainWorktreePath,
            newWorktreePath: payload.newWorktreePath,
          },
          (progress) => {
            if (!sender.isDestroyed()) {
              sender.send('worktree:setupProgress', progress)
            }
          },
          controller.signal,
        )
      } finally {
        setupAbortControllers.delete(sender.id)
      }
    },
  )

  ipcMain.on('worktree:abortSetup', (event) => {
    const controller = setupAbortControllers.get(event.sender.id)
    controller?.abort()
  })

  // --- Onboarding ---

  ipcMain.handle('onboarding:getCompleted', () => {
    return onboardingStore.getCompleted()
  })

  ipcMain.handle(
    'onboarding:complete',
    (_event, payload: { stepIds: string[]; appVersion: string }) => {
      if (!Array.isArray(payload.stepIds) || typeof payload.appVersion !== 'string') return
      if (payload.stepIds.length === 0 || !payload.appVersion) return
      const safeIds = payload.stepIds.filter(
        (id) => typeof id === 'string' && id.length > 0 && id.length < 100,
      )
      if (safeIds.length === 0) return
      onboardingStore.completeMany(safeIds, payload.appVersion)
    },
  )

  ipcMain.handle('onboarding:reset', () => {
    onboardingStore.reset()
  })
}
