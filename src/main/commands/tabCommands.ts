import { BrowserWindow, type WebContents } from 'electron'
import os from 'os'
import { randomUUID } from 'crypto'
import type { PtyManager } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'
import type { WorkspaceStore } from '../db/WorkspaceStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { LayoutStore } from '../db/LayoutStore'
import type { ToolRegistry } from '../tools/ToolRegistry'
import type { AgentSessionManager } from '../agents/AgentSessionManager'
import type { WindowManager } from '../WindowManager'
import type { BrowserManager } from '../browser/BrowserManager'
import type { TmuxManager } from '../pty/TmuxManager'
import { TmuxManager as TmuxManagerStatics } from '../pty/TmuxManager'
import type { ProfileStore } from '../profiles/ProfileStore'
import { profileToReader } from '../profiles/ProfileStore'
import { profileErrorMessage } from '../profiles/errors'
import type { PreferencesReader } from '../agents/types'
import type { PaneSnapshot, SplitSnapshot, TabCommandResult, TabSnapshot } from './types'

export interface ToolSpawnPayload {
  toolId: string
  worktreePath: string
  cols?: number
  rows?: number
  workspaceName?: string
  branch?: string
  resumeSessionId?: string
  profileId?: string
}

export interface ToolSpawnResult {
  sessionId: string
  wsUrl: string
  toolId: string
  toolName: string
  tmuxSessionName?: string
}

interface TmuxAttachPayload {
  tmuxSessionName: string
  cols?: number
  rows?: number
}

interface ToolSessionServiceDeps {
  ptyManager: PtyManager
  wsBridge: WsBridge
  workspaceStore: WorkspaceStore
  preferencesStore: PreferencesStore
  toolRegistry: ToolRegistry
  agentSessionManager: AgentSessionManager
  windowManager: WindowManager
  tmuxManager: TmuxManager
  profileStore: ProfileStore
}

function resolveShellArgs(): string[] {
  if (os.platform() === 'win32') return []
  return ['--login']
}

function validateTmuxName(name: string): void {
  if (!/^[\w-]+$/.test(name)) {
    throw new Error('Invalid tmux session name: only letters, digits, underscores, and dashes')
  }
}

export class ToolSessionService {
  constructor(private deps: ToolSessionServiceDeps) {}

  async spawnTool(sender: WebContents, payload: ToolSpawnPayload): Promise<ToolSpawnResult> {
    const tool = this.deps.toolRegistry.get(payload.toolId)
    if (!tool) throw new Error(`Unknown tool: ${payload.toolId}`)

    let command = this.deps.toolRegistry.resolveCommand(tool)
    const isShell = tool.id === 'shell' || tool.command === 'shell'
    const isAgent = this.deps.agentSessionManager.isAgentTool(tool.id)
    let args = isShell ? resolveShellArgs() : [...tool.args]
    let env: Record<string, string> | undefined

    let agentTempId: string | undefined
    if (isAgent) {
      const senderWindow = BrowserWindow.fromWebContents(sender)
      if (!senderWindow) throw new Error('No window for agent session')

      let prefsReader: PreferencesReader = this.deps.preferencesStore
      if (payload.profileId) {
        const profileResult = await this.deps.profileStore.getInternal(payload.profileId)
        if (profileResult.isErr()) {
          throw new Error(profileErrorMessage(profileResult.error))
        }
        const profile = profileResult.value
        if (profile.agentType !== tool.id) {
          throw new Error(`Profile ${profile.name} is for ${profile.agentType}, not ${tool.id}`)
        }
        prefsReader = profileToReader(profile, this.deps.preferencesStore)
      }

      let settingsOverrides: Record<string, unknown> | undefined
      const settingsJsonRaw = prefsReader.get(`${tool.id}.settingsJson`)
      if (settingsJsonRaw) {
        try {
          settingsOverrides = JSON.parse(settingsJsonRaw) as Record<string, unknown>
        } catch {
          // Invalid JSON is ignored, matching the legacy tool:spawn path.
        }
      }

      const agentSession = await this.deps.agentSessionManager.createSession(
        tool.id,
        payload.worktreePath,
        payload.workspaceName ?? '',
        payload.branch ?? null,
        senderWindow,
        settingsOverrides,
      )
      args = [...agentSession.settingsArgs, ...args]
      if (payload.resumeSessionId) {
        args.push(...this.deps.agentSessionManager.getResumeArgs(tool.id, payload.resumeSessionId))
      }
      args.push(...this.deps.agentSessionManager.getCliArgs(tool.id, prefsReader))
      env = {
        CANOPY_HOOK_PORT: String(agentSession.hookPort),
        CANOPY_HOOK_PATH: agentSession.hookPath,
        CANOPY_HOOK_TOKEN: agentSession.hookAuthToken,
        ...agentSession.settingsEnv,
        ...this.deps.agentSessionManager.getEnvVars(tool.id, prefsReader),
      }
      agentTempId = agentSession.tempId
    }

    let tmuxSessionName: string | undefined
    const tmuxEnabled = this.deps.preferencesStore.get('tmux.enabled') === 'true'
    if (tmuxEnabled && (await this.deps.tmuxManager.isAvailable())) {
      const ws = this.deps.workspaceStore.getByPath(payload.worktreePath)
      const wsId = ws?.id ?? 'default'
      tmuxSessionName = TmuxManagerStatics.sessionName(wsId)
      const tmuxMouse = this.deps.preferencesStore.get('tmux.mouse') === 'true'
      await this.deps.tmuxManager.newSession({
        name: tmuxSessionName,
        cwd: payload.worktreePath,
        shell: command,
        shellArgs: args,
        cols: payload.cols,
        rows: payload.rows,
        mouse: tmuxMouse,
        env,
      })
      const attach = this.deps.tmuxManager.attachArgs(tmuxSessionName)
      command = attach.command
      args = attach.args
    }

    const session = this.deps.ptyManager.spawn({
      command,
      args,
      cwd: payload.worktreePath,
      cols: payload.cols,
      rows: payload.rows,
      env,
      tmuxSessionName,
    })

    if (isAgent && agentTempId) {
      this.deps.agentSessionManager.rekey(agentTempId, session.id)
    }

    const wsUrl = await this.deps.wsBridge.create(session.id, session.pty)

    this.deps.windowManager.trackPtySession(sender.id, session.id)

    session.pty.onExit(({ exitCode, signal }) => {
      if (!sender.isDestroyed()) {
        sender.send('pty:exit', {
          sessionId: session.id,
          exitCode,
          signal,
          tmuxSessionName: session.tmuxSessionName,
        })
      }
      this.deps.windowManager.untrackPtySession(sender.id, session.id)
      if (isAgent) {
        this.deps.agentSessionManager.destroySession(session.id)
      }
    })

    return {
      sessionId: session.id,
      wsUrl,
      toolId: tool.id,
      toolName: tool.name,
      tmuxSessionName,
    }
  }

  async attachTmux(
    sender: WebContents,
    payload: TmuxAttachPayload,
  ): Promise<{
    sessionId: string
    wsUrl: string
  }> {
    validateTmuxName(payload.tmuxSessionName)
    const attach = this.deps.tmuxManager.attachArgs(payload.tmuxSessionName)
    const session = this.deps.ptyManager.spawn({
      command: attach.command,
      args: attach.args,
      cols: payload.cols,
      rows: payload.rows,
      tmuxSessionName: payload.tmuxSessionName,
    })
    const wsUrl = await this.deps.wsBridge.create(session.id, session.pty)

    this.deps.windowManager.trackPtySession(sender.id, session.id)

    session.pty.onExit(({ exitCode, signal }) => {
      if (!sender.isDestroyed()) {
        sender.send('pty:exit', {
          sessionId: session.id,
          exitCode,
          signal,
          tmuxSessionName: payload.tmuxSessionName,
        })
      }
      this.deps.windowManager.untrackPtySession(sender.id, session.id)
    })

    return { sessionId: session.id, wsUrl }
  }

  async killPty(sessionId: string, killTmux?: boolean): Promise<void> {
    const tmuxName = this.deps.ptyManager.getTmuxSessionName(sessionId)
    if (killTmux && tmuxName && TmuxManagerStatics.isCanopySession(tmuxName)) {
      try {
        await this.deps.tmuxManager.killSession(tmuxName)
      } catch {
        // Session may already be gone.
      }
    }
    this.deps.wsBridge.destroy(sessionId)
    this.deps.ptyManager.kill(sessionId)
  }

  isAgentTool(toolId: string): boolean {
    return this.deps.agentSessionManager.isAgentTool(toolId)
  }

  destroyAgentSession(sessionId: string): void {
    this.deps.agentSessionManager.destroySession(sessionId)
  }

  getToolName(toolId: string): string {
    return this.deps.toolRegistry.get(toolId)?.name ?? toolId
  }

  async getProfileName(profileId: string): Promise<string | undefined> {
    const result = await this.deps.profileStore.getInternal(profileId)
    return result.isOk() ? result.value.name : undefined
  }

  async hasTmuxSession(name: string): Promise<boolean> {
    validateTmuxName(name)
    return this.deps.tmuxManager.hasSession(name)
  }
}

interface TabCommandServiceDeps {
  toolSessions: ToolSessionService
  layoutStore: LayoutStore
  workspaceStore: WorkspaceStore
  browserManager: BrowserManager
  windowManager: WindowManager
}

interface TabCommandPayloadBase {
  worktreePath: string
  tabs?: TabSnapshot[]
  activeTabId?: string | null
}

interface OpenToolPayload extends TabCommandPayloadBase {
  toolId: string
  options?: {
    initialUrl?: string
    profileId?: string
    workspaceName?: string
    branch?: string
  }
}

interface RestartPanePayload extends TabCommandPayloadBase {
  tabId: string
  paneId: string
  options?: {
    workspaceName?: string
    branch?: string
  }
}

interface CloseTabPayload extends TabCommandPayloadBase {
  tabId: string
}

interface SaveLayoutPayload {
  worktreePath: string
  layoutJson: string
  workspaceId?: string
}

function allPaneSnapshots(split: SplitSnapshot): PaneSnapshot[] {
  validateSplitSnapshot(split)
  if (split.type === 'leaf') return [split.pane]
  return [...allPaneSnapshots(split.first), ...allPaneSnapshots(split.second)]
}

function updatePaneSnapshot(
  split: SplitSnapshot,
  paneId: string,
  updater: (pane: PaneSnapshot) => PaneSnapshot,
): SplitSnapshot {
  if (split.type === 'leaf') {
    if (split.pane.id !== paneId) return split
    return { type: 'leaf', pane: updater(split.pane) }
  }
  return {
    ...split,
    first: updatePaneSnapshot(split.first, paneId, updater),
    second: updatePaneSnapshot(split.second, paneId, updater),
  }
}

function computeDisplayName(
  toolName: string,
  worktreePath: string,
  toolId: string,
  tabs: TabSnapshot[],
  profileName?: string,
): string {
  const baseLabel =
    profileName && profileName !== 'Default' ? `${toolName} (${profileName})` : toolName
  const sameLabelCount = tabs.filter(
    (t) =>
      t.worktreePath === worktreePath &&
      (t.name === baseLabel || t.name.startsWith(`${baseLabel} #`)) &&
      t.toolId === toolId,
  ).length
  if (sameLabelCount === 0) return baseLabel
  return `${baseLabel} #${sameLabelCount + 1}`
}

function tabId(): string {
  return `tab-${randomUUID()}`
}

function paneId(): string {
  return `pane-${randomUUID()}`
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function assertString(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string') throw new Error(`Invalid tab snapshot: ${label} must be a string`)
}

function assertOptionalString(value: unknown, label: string): asserts value is string | undefined {
  if (value !== undefined && typeof value !== 'string') {
    throw new Error(`Invalid tab snapshot: ${label} must be a string`)
  }
}

function validatePaneSnapshot(value: unknown): asserts value is PaneSnapshot {
  if (!isRecord(value)) throw new Error('Invalid tab snapshot: pane must be an object')
  assertString(value.id, 'pane.id')
  assertString(value.sessionId, 'pane.sessionId')
  assertString(value.wsUrl, 'pane.wsUrl')
  assertString(value.toolId, 'pane.toolId')
  assertString(value.toolName, 'pane.toolName')
  if (typeof value.isRunning !== 'boolean') {
    throw new Error('Invalid tab snapshot: pane.isRunning must be a boolean')
  }
  if (value.exitCode !== null && typeof value.exitCode !== 'number') {
    throw new Error('Invalid tab snapshot: pane.exitCode must be a number or null')
  }
  if (value.title !== null && typeof value.title !== 'string') {
    throw new Error('Invalid tab snapshot: pane.title must be a string or null')
  }
  assertOptionalString(value.paneType, 'pane.paneType')
  assertOptionalString(value.tmuxSessionName, 'pane.tmuxSessionName')
  assertOptionalString(value.url, 'pane.url')
  assertOptionalString(value.filePath, 'pane.filePath')
  assertOptionalString(value.editorActiveFile, 'pane.editorActiveFile')
  assertOptionalString(value.profileId, 'pane.profileId')
  assertOptionalString(value.profileName, 'pane.profileName')
  if (value.detached !== undefined && typeof value.detached !== 'boolean') {
    throw new Error('Invalid tab snapshot: pane.detached must be a boolean')
  }
}

function validateSplitSnapshot(value: unknown): asserts value is SplitSnapshot {
  if (!isRecord(value)) throw new Error('Invalid tab snapshot: split must be an object')
  if (value.type === 'leaf') {
    validatePaneSnapshot(value.pane)
    return
  }
  if (value.type !== 'split') throw new Error('Invalid tab snapshot: split.type is invalid')
  if (value.direction !== 'horizontal' && value.direction !== 'vertical') {
    throw new Error('Invalid tab snapshot: split.direction is invalid')
  }
  if (typeof value.ratio !== 'number' || !Number.isFinite(value.ratio)) {
    throw new Error('Invalid tab snapshot: split.ratio must be a finite number')
  }
  validateSplitSnapshot(value.first)
  validateSplitSnapshot(value.second)
}

function validateTabSnapshots(value: unknown): TabSnapshot[] {
  if (!Array.isArray(value)) return []
  for (const tab of value) {
    if (!isRecord(tab)) throw new Error('Invalid tab snapshot: tab must be an object')
    assertString(tab.id, 'tab.id')
    assertString(tab.toolId, 'tab.toolId')
    assertString(tab.toolName, 'tab.toolName')
    assertString(tab.name, 'tab.name')
    assertString(tab.worktreePath, 'tab.worktreePath')
    assertString(tab.focusedPaneId, 'tab.focusedPaneId')
    validateSplitSnapshot(tab.rootSplit)
  }
  return value
}

function emptyResult(
  worktreePath: string,
  tabs: TabSnapshot[],
  activeTabId: string | null,
): TabCommandResult {
  return { worktreePath, tabs, activeTabId }
}

export class TabCommandService {
  constructor(private deps: TabCommandServiceDeps) {}

  async openTool(sender: WebContents, payload: OpenToolPayload): Promise<TabCommandResult> {
    const tabs = [...validateTabSnapshots(payload.tabs)]
    const pane = await this.createPane(sender, payload.toolId, payload.worktreePath, {
      ...payload.options,
    })
    const id = tabId()
    const openedTab: TabSnapshot = {
      id,
      toolId: pane.toolId,
      toolName: pane.toolName,
      name: computeDisplayName(
        pane.toolName,
        payload.worktreePath,
        pane.toolId,
        tabs,
        pane.profileName,
      ),
      worktreePath: payload.worktreePath,
      rootSplit: { type: 'leaf', pane },
      focusedPaneId: pane.id,
    }

    tabs.push(openedTab)
    return {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: id,
      openedTab,
    }
  }

  async restartPane(sender: WebContents, payload: RestartPanePayload): Promise<TabCommandResult> {
    const tabs = [...validateTabSnapshots(payload.tabs)]
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    if (tabIndex < 0) {
      return emptyResult(payload.worktreePath, tabs, payload.activeTabId ?? null)
    }

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, payload.activeTabId ?? null)

    const restartedPane = await this.restartPaneSnapshot(sender, payload.worktreePath, pane, {
      workspaceName: payload.options?.workspaceName,
      branch: payload.options?.branch,
    })

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, () => restartedPane),
    }

    return {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: payload.activeTabId ?? tab.id,
      restartedPane,
    }
  }

  async closeTab(sender: WebContents, payload: CloseTabPayload): Promise<TabCommandResult> {
    const tabs = [...validateTabSnapshots(payload.tabs)]
    const idx = tabs.findIndex((tab) => tab.id === payload.tabId)
    if (idx < 0) return emptyResult(payload.worktreePath, tabs, payload.activeTabId ?? null)

    const [tab] = tabs.splice(idx, 1)
    await this.cleanupPanes(sender, allPaneSnapshots(tab.rootSplit))

    let nextActiveId = payload.activeTabId ?? null
    if (nextActiveId === payload.tabId) {
      nextActiveId = tabs.length > 0 ? tabs[Math.min(idx, tabs.length - 1)].id : null
    }

    return {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: nextActiveId,
      closedTabId: payload.tabId,
    }
  }

  saveLayout(payload: SaveLayoutPayload): void {
    const workspaceId =
      payload.workspaceId ?? this.deps.workspaceStore.getByPath(payload.worktreePath)?.id
    if (!workspaceId) return

    try {
      this.deps.layoutStore.save(workspaceId, payload.worktreePath, payload.layoutJson)
    } catch (error) {
      if (this.deps.layoutStore.isClosed()) return
      console.error('Failed to save layout:', error)
    }
  }

  restoreLayout(payload: TabCommandPayloadBase & { layoutJson: string }): TabCommandResult {
    try {
      JSON.parse(payload.layoutJson)
    } catch {
      return emptyResult(payload.worktreePath, payload.tabs ?? [], payload.activeTabId ?? null)
    }
    return emptyResult(payload.worktreePath, payload.tabs ?? [], payload.activeTabId ?? null)
  }

  private async createPane(
    sender: WebContents,
    toolId: string,
    worktreePath: string,
    options?: {
      initialUrl?: string
      profileId?: string
      workspaceName?: string
      branch?: string
      resumeSessionId?: string
    },
  ): Promise<PaneSnapshot> {
    if (toolId === 'browser') {
      return {
        id: paneId(),
        sessionId: randomUUID(),
        wsUrl: '',
        toolId,
        toolName: 'Browser',
        isRunning: true,
        exitCode: null,
        title: null,
        paneType: 'browser',
        url: options?.initialUrl,
      }
    }

    if (toolId === 'notes' || toolId === 'drawing') {
      return {
        id: paneId(),
        sessionId: randomUUID(),
        wsUrl: '',
        toolId,
        toolName: toolId === 'notes' ? 'Notes' : 'Drawing',
        isRunning: true,
        exitCode: null,
        title: null,
        paneType: toolId,
      }
    }

    if (toolId === 'editor') {
      return {
        id: paneId(),
        sessionId: '',
        wsUrl: '',
        toolId,
        toolName: 'Editor',
        isRunning: true,
        exitCode: null,
        title: null,
        paneType: 'editor',
      }
    }

    const result = await this.deps.toolSessions.spawnTool(sender, {
      toolId,
      worktreePath,
      workspaceName: options?.workspaceName,
      branch: options?.branch,
      resumeSessionId: options?.resumeSessionId,
      profileId: options?.profileId,
    })

    let profileName: string | undefined
    if (options?.profileId) {
      profileName = await this.deps.toolSessions.getProfileName(options.profileId)
    }

    return {
      id: paneId(),
      sessionId: result.sessionId,
      wsUrl: result.wsUrl,
      toolId,
      toolName: result.toolName,
      isRunning: true,
      exitCode: null,
      title: null,
      tmuxSessionName: result.tmuxSessionName,
      profileId: options?.profileId,
      profileName,
    }
  }

  private async restartPaneSnapshot(
    sender: WebContents,
    worktreePath: string,
    pane: PaneSnapshot,
    options: { workspaceName?: string; branch?: string },
  ): Promise<PaneSnapshot> {
    if (pane.paneType === 'editor' || pane.paneType === 'diff') {
      return pane
    }

    if (pane.paneType === 'browser') {
      try {
        this.deps.browserManager.teardown(pane.sessionId)
      } catch {
        // Already destroyed.
      }
      return {
        ...pane,
        sessionId: randomUUID(),
        isRunning: true,
        exitCode: null,
        title: null,
      }
    }

    if (pane.paneType === 'notes' || pane.paneType === 'drawing') {
      return {
        ...pane,
        sessionId: randomUUID(),
        isRunning: true,
        exitCode: null,
        title: null,
      }
    }

    if (pane.tmuxSessionName && pane.detached) {
      const exists = await this.deps.toolSessions
        .hasTmuxSession(pane.tmuxSessionName)
        .catch(() => false)
      if (exists) {
        const result = await this.deps.toolSessions.attachTmux(sender, {
          tmuxSessionName: pane.tmuxSessionName,
        })
        return {
          ...pane,
          sessionId: result.sessionId,
          wsUrl: result.wsUrl,
          isRunning: true,
          exitCode: null,
          detached: false,
        }
      }
    }

    if (pane.sessionId) {
      if (this.deps.windowManager.ownsPtySession(sender.id, pane.sessionId)) {
        try {
          await this.deps.toolSessions.killPty(pane.sessionId)
        } catch {
          // Already exited or cleaned up.
        }
        if (this.deps.toolSessions.isAgentTool(pane.toolId)) {
          this.deps.toolSessions.destroyAgentSession(pane.sessionId)
        }
      }
    }

    const result = await this.deps.toolSessions.spawnTool(sender, {
      toolId: pane.toolId,
      worktreePath,
      workspaceName: options.workspaceName,
      branch: options.branch,
      profileId: pane.profileId,
    })

    return {
      ...pane,
      sessionId: result.sessionId,
      wsUrl: result.wsUrl,
      isRunning: true,
      exitCode: null,
      title: null,
      detached: false,
      tmuxSessionName: result.tmuxSessionName,
    }
  }

  private async cleanupPanes(sender: WebContents, panes: PaneSnapshot[]): Promise<void> {
    await Promise.all(
      panes.map(async (pane) => {
        if (
          pane.paneType === 'editor' ||
          pane.paneType === 'diff' ||
          pane.paneType === 'notes' ||
          pane.paneType === 'drawing'
        ) {
          return
        }

        if (pane.paneType === 'browser') {
          if (pane.sessionId) this.deps.browserManager.teardown(pane.sessionId)
          return
        }

        if (pane.sessionId && this.deps.windowManager.ownsPtySession(sender.id, pane.sessionId)) {
          if (this.deps.toolSessions.isAgentTool(pane.toolId)) {
            this.deps.toolSessions.destroyAgentSession(pane.sessionId)
          }
          await this.deps.toolSessions.killPty(pane.sessionId, !!pane.tmuxSessionName)
        }
      }),
    )
  }
}
