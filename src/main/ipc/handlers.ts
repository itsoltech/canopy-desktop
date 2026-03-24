import { ipcMain, dialog, shell, BrowserWindow } from 'electron'
import os from 'os'
import type { PtyManager } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { LayoutStore } from '../db/LayoutStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import type { ClaudeSessionManager } from '../claude/ClaudeSessionManager'
import type { WindowManager } from '../WindowManager'
import { GitRepository } from '../git/GitRepository'
import { GitWatcher } from '../git/GitWatcher'
import { runWorktreeSetup } from '../worktree/WorktreeSetupRunner'
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
  claudeSessionManager: ClaudeSessionManager,
  windowManager: WindowManager,
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
      const isClaude = tool.id === 'claude'
      let args = isShell ? resolveShellArgs() : [...tool.args]
      let env: Record<string, string> | undefined

      let claudeTempId: string | undefined
      if (isClaude) {
        const senderWindow = BrowserWindow.fromWebContents(sender)
        if (!senderWindow) throw new Error('No window for Claude session')

        // Parse settings.json overrides from prefs
        let settingsOverrides: Record<string, unknown> | undefined
        const settingsJsonRaw = preferencesStore.get('claude.settingsJson')
        if (settingsJsonRaw) {
          try {
            settingsOverrides = JSON.parse(settingsJsonRaw) as Record<string, unknown>
          } catch {
            // Invalid JSON
          }
        }

        const claudeSession = await claudeSessionManager.createSession(
          payload.worktreePath,
          payload.workspaceName ?? '',
          payload.branch ?? null,
          senderWindow,
          settingsOverrides,
        )
        args = ['--settings', claudeSession.settingsPath, ...args]
        if (payload.resumeSessionId) args.push('--resume', payload.resumeSessionId)
        env = { CANOPY_HOOK_PORT: String(claudeSession.hookPort) }
        claudeTempId = claudeSession.tempId

        // CLI args from preferences
        const claudeModel = preferencesStore.get('claude.model')
        const claudePermMode = preferencesStore.get('claude.permissionMode')
        const claudeEffort = preferencesStore.get('claude.effortLevel')
        const claudeAppendPrompt = preferencesStore.get('claude.appendSystemPrompt')
        if (claudeModel) args.push('--model', claudeModel)
        if (claudePermMode) args.push('--permission-mode', claudePermMode)
        if (claudeEffort) args.push('--effort', claudeEffort)
        if (claudeAppendPrompt) args.push('--append-system-prompt', claudeAppendPrompt)

        // Env vars from preferences
        const claudeApiKey = preferencesStore.get('claude.apiKey')
        const claudeBaseUrl = preferencesStore.get('claude.baseUrl')
        const claudeProvider = preferencesStore.get('claude.provider')
        const claudeCustomEnv = preferencesStore.get('claude.customEnv')
        if (claudeApiKey) env.ANTHROPIC_API_KEY = claudeApiKey
        if (claudeBaseUrl) env.ANTHROPIC_BASE_URL = claudeBaseUrl
        if (claudeProvider === 'bedrock') env.CLAUDE_CODE_USE_BEDROCK = '1'
        if (claudeProvider === 'vertex') env.CLAUDE_CODE_USE_VERTEX = '1'
        if (claudeProvider === 'foundry') env.CLAUDE_CODE_USE_FOUNDRY = '1'
        if (claudeCustomEnv) {
          try {
            Object.assign(env, JSON.parse(claudeCustomEnv))
          } catch {
            // Invalid JSON
          }
        }
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

      windowManager.trackPtySession(sender.id, session.id)

      session.pty.onExit(({ exitCode, signal }) => {
        if (!sender.isDestroyed()) {
          sender.send('pty:exit', { sessionId: session.id, exitCode, signal })
        }
        windowManager.untrackPtySession(sender.id, session.id)
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

  ipcMain.handle('app:homedir', () => os.homedir())

  ipcMain.handle('app:showInFolder', (_event, payload: { path: string }) => {
    shell.showItemInFolder(payload.path)
  })

  // --- App: Multi-window ---

  ipcMain.handle('app:newWindow', () => {
    windowManager.createWindow()
  })

  ipcMain.handle('app:setWorkspacePath', (event, payload: { path: string }) => {
    windowManager.setWorkspacePath(event.sender.id, payload.path)
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

    // Dispose previous watcher for this window only
    windowManager.disposeGitWatcher(senderId)

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
        event.sender.send('git:changed', info)
      }
    })
    watcher.start()
    windowManager.setGitWatcher(senderId, watcher)
  })

  ipcMain.handle('git:unwatch', (event) => {
    windowManager.disposeGitWatcher(event.sender.id)
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

  // --- Worktree Setup ---

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
      return runWorktreeSetup(
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
      )
    },
  )
}
