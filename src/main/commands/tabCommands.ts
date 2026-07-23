import { BrowserWindow, type WebContents } from 'electron'
import { randomUUID } from 'crypto'
import { resolveShell, type PtyManager } from '../pty/PtyManager'
import type { TerminalStreamService } from '../pty/TerminalStreamService'
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
import type {
  CloseWarningResult,
  CloseWarningTarget,
  EditorFileLoadResult,
  EditorFileReadResult,
  EditorFileSaveResult,
  EditorFileSnapshot,
  PaneSnapshot,
  SerializedLayout,
  SerializedSplitNode,
  SplitSnapshot,
  TabCloseAllPreflightResult,
  TabClosePreflightResult,
  TabCommandResult,
  TabSnapshot,
  TabStateSnapshot,
} from './types'

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
  terminalStreamService: TerminalStreamService
  preferencesStore: PreferencesStore
  toolRegistry: ToolRegistry
  agentSessionManager: AgentSessionManager
  windowManager: WindowManager
  tmuxManager: TmuxManager
  profileStore: ProfileStore
  resolveWorkspaceIdForWorktree: (webContentsId: number, worktreePath: string) => string | null
}

function validateTmuxName(name: string): void {
  if (!/^[\w-]+$/.test(name)) {
    throw new Error('Invalid tmux session name: only letters, digits, underscores, and dashes')
  }
}

export class ToolSessionService {
  constructor(private deps: ToolSessionServiceDeps) {}

  async spawnTool(sender: WebContents, payload: ToolSpawnPayload): Promise<ToolSpawnResult> {
    const workspaceId = this.deps.resolveWorkspaceIdForWorktree(sender.id, payload.worktreePath)
    if (!workspaceId) {
      throw new Error(`Worktree is not attached to this window: ${payload.worktreePath}`)
    }

    const tool = this.deps.toolRegistry.get(payload.toolId)
    if (!tool) throw new Error(`Unknown tool: ${payload.toolId}`)

    let command = this.deps.toolRegistry.resolveCommand(tool)
    const isShell = tool.id === 'shell' || tool.command === 'shell'
    const isAgent = this.deps.agentSessionManager.isAgentTool(tool.id)
    let args = isShell ? resolveShell().args : [...tool.args]
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
          // Parsed settings are an opaque override bag forwarded to the agent
          // adapter, which validates the shape it cares about; treat as a record here.
          settingsOverrides = JSON.parse(settingsJsonRaw) as Record<string, unknown>
        } catch {
          // Invalid JSON is ignored, matching the previous spawn behavior.
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
      tmuxSessionName = TmuxManagerStatics.sessionName(workspaceId)
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

    try {
      this.deps.windowManager.trackPtySession(sender.id, session.id)
      this.deps.terminalStreamService.register(session.id, session.pty, sender.id)
    } catch (error) {
      this.deps.terminalStreamService.destroy(session.id)
      this.deps.windowManager.untrackPtySession(sender.id, session.id)
      this.deps.ptyManager.kill(session.id)
      if (isAgent) this.deps.agentSessionManager.destroySession(session.id)
      throw error
    }

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
      wsUrl: '',
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

    try {
      this.deps.windowManager.trackPtySession(sender.id, session.id)
      this.deps.terminalStreamService.register(session.id, session.pty, sender.id)
    } catch (error) {
      this.deps.terminalStreamService.destroy(session.id)
      this.deps.windowManager.untrackPtySession(sender.id, session.id)
      this.deps.ptyManager.kill(session.id)
      throw error
    }

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

    return { sessionId: session.id, wsUrl: '' }
  }

  async killPty(
    sessionId: string,
    killTmux?: boolean,
    options?: { treeWait?: boolean },
  ): Promise<void> {
    const tmuxName = this.deps.ptyManager.getTmuxSessionName(sessionId)
    this.deps.terminalStreamService.destroy(sessionId)
    if (killTmux && tmuxName && TmuxManagerStatics.isCanopySession(tmuxName)) {
      try {
        await this.deps.tmuxManager.killSession(tmuxName)
      } catch {
        // Session may already be gone.
      }
    }
    if (options?.treeWait) {
      // Removal flow: the session record must survive until the whole process tree
      // is dead, or the directory being deleted still holds live cwd handles.
      await this.deps.ptyManager.killAndWait(sessionId)
    } else {
      this.deps.ptyManager.kill(sessionId)
    }
  }

  isAgentTool(toolId: string): boolean {
    return this.deps.agentSessionManager.isAgentTool(toolId)
  }

  isAgentBusy(sessionId: string): boolean {
    return this.deps.agentSessionManager.isBusy(sessionId)
  }

  hasChildProcess(sessionId: string): Promise<boolean> {
    return this.deps.ptyManager.hasChildProcess(sessionId)
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

  getAgentSessionId(ptySessionId: string): string | undefined {
    return this.deps.agentSessionManager.getSession(ptySessionId)?.agentSessionId
  }

  getOwnedSessionConnection(
    sender: WebContents,
    sessionId: string,
  ): { wsUrl: string; tmuxSessionName?: string } {
    if (!this.deps.windowManager.ownsPtySession(sender.id, sessionId)) {
      throw new Error('PTY session is not owned by this window')
    }

    return {
      wsUrl: '',
      tmuxSessionName: this.deps.ptyManager.getTmuxSessionName(sessionId),
    }
  }

  async hasTmuxSession(name: string): Promise<boolean> {
    validateTmuxName(name)
    return this.deps.tmuxManager.hasSession(name)
  }

  async killTmuxSession(name: string): Promise<void> {
    validateTmuxName(name)
    if (!TmuxManagerStatics.isCanopySession(name)) return
    await this.deps.tmuxManager.killSession(name)
  }
}

interface TabCommandServiceDeps {
  toolSessions: ToolSessionService
  layoutStore: LayoutStore
  browserManager: BrowserManager
  windowManager: WindowManager
  confirmUnsavedChanges: (
    sender: WebContents,
    filePaths: string[],
  ) => Promise<'save' | 'discard' | 'cancel'>
  writeEditorFile: (
    sender: WebContents,
    filePath: string,
    content: string,
    expectedMtimeMs?: number,
  ) => Promise<EditorWriteFileResult>
  loadEditorFile: (
    sender: WebContents,
    filePath: string,
    maxBytes?: number,
  ) => Promise<EditorFileReadResult>
  resolveWorkspaceIdForWorktree: (webContentsId: number, worktreePath: string) => string | null
  emitAppStateChanged: (sender: WebContents) => void
}

interface TabCommandPayloadBase {
  worktreePath: string
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

interface OpenDiffPayload extends TabCommandPayloadBase {}

interface OpenSessionTabPayload extends TabCommandPayloadBase {
  name: string
  sessionId: string
}

interface OpenEditorFilePayload extends TabCommandPayloadBase {
  filePath: string
}

interface DetachEditorFilePayload extends TabCommandPayloadBase {
  paneId: string
  filePath: string
}

interface CloseEditorFilePayload extends TabCommandPayloadBase {
  paneId: string
  filePath: string
}

interface PrepareCloseEditorFilePayload extends TabCommandPayloadBase {
  paneId: string
  filePath: string
}

interface MoveEditorFilePayload extends TabCommandPayloadBase {
  paneId: string
  filePath: string
  toIndex: number
}

interface MoveEditorFileBetweenPanesPayload extends TabCommandPayloadBase {
  sourcePaneId: string
  targetPaneId: string
  filePath: string
  toIndex: number
}

interface SetActiveEditorFilePayload extends TabCommandPayloadBase {
  paneId: string
  filePath: string
}

interface UpdateEditorFileStatePayload extends TabCommandPayloadBase {
  paneId: string
  filePath: string
  patch: Partial<EditorFileSnapshot>
}

interface SaveEditorFilePayload extends TabCommandPayloadBase {
  paneId: string
  filePath: string
  options: {
    content: string
    fileLineEnding?: 'LF' | 'CRLF'
    expectedMtimeMs?: number
  }
}

interface LoadEditorFilePayload extends TabCommandPayloadBase {
  paneId: string
  filePath: string
  options?: {
    maxBytes?: number
  }
}

interface UpdatePaneTitlePayload extends TabCommandPayloadBase {
  sessionId: string
  title: string
}

interface UpdatePaneUrlPayload extends TabCommandPayloadBase {
  sessionId: string
  url: string
}

interface UpdateTmuxSessionNamePayload extends TabCommandPayloadBase {
  oldName: string
  newName: string
}

interface HandlePtyExitPayload extends TabCommandPayloadBase {
  sessionId: string
  exitCode: number
  tmuxSessionName?: string
}

interface KillTmuxPanePayload extends TabCommandPayloadBase {
  tabId: string
  paneId: string
}

interface ReattachTmuxPanePayload extends TabCommandPayloadBase {
  tabId: string
  paneId: string
  options?: {
    workspaceName?: string
    branch?: string
  }
}

interface ToggleFocusedInspectorPayload extends TabCommandPayloadBase {
  tabId: string
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

interface PrepareCloseTabPayload extends TabCommandPayloadBase {
  tabId: string
}

interface PrepareCloseAllForWorktreePayload extends TabCommandPayloadBase {
  confirmedActiveProcesses?: boolean
}

interface GetCloseWarningPayload extends TabCommandPayloadBase {
  target: CloseWarningTarget
}

interface ClosePanePayload extends TabCommandPayloadBase {
  tabId: string
  paneId: string
}

interface CloseAllForWorktreePayload extends TabCommandPayloadBase {
  /** Worktree-removal variant: tree-kill PTYs and WAIT for process exit so the
   *  directory holds no live cwd/file handles when `git worktree remove` runs. */
  forRemoval?: boolean
}

interface ReopenClosedTabPayload extends TabCommandPayloadBase {
  options?: {
    workspaceName?: string
    branch?: string
  }
}

interface SetActiveTabPayload extends TabCommandPayloadBase {
  tabId: string
}

interface MoveTabPayload extends TabCommandPayloadBase {
  fromIndex: number
  toIndex: number
}

interface MoveTabToSplitPayload extends TabCommandPayloadBase {
  sourceTabId: string
  targetTabId: string
  targetPaneId: string
  direction: 'horizontal' | 'vertical'
  position: 'first' | 'second'
}

interface MovePaneToTargetPayload extends TabCommandPayloadBase {
  sourceTabId: string
  sourcePaneId: string
  targetTabId: string
  targetPaneId: string
  direction: 'horizontal' | 'vertical'
  position: 'first' | 'second'
}

interface DetachPaneToTabPayload extends TabCommandPayloadBase {
  sourceTabId: string
  sourcePaneId: string
}

interface SpawnPanePayload extends TabCommandPayloadBase {
  toolId: string
  options?: {
    initialUrl?: string
    profileId?: string
    workspaceName?: string
    branch?: string
    resumeSessionId?: string
  }
}

interface SplitPanePayload extends TabCommandPayloadBase {
  tabId: string
  paneId: string
  direction: 'horizontal' | 'vertical'
}

interface FocusPanePayload extends TabCommandPayloadBase {
  tabId: string
  paneId: string
}

interface NavigatePaneFocusPayload extends TabCommandPayloadBase {
  tabId: string
  direction: 'left' | 'right' | 'up' | 'down'
}

interface UpdateSplitRatioPayload extends TabCommandPayloadBase {
  tabId: string
  splitId: string
  ratio: number
}

interface RestoreLayoutPayload extends TabCommandPayloadBase {
  layoutJson: string
  options?: {
    workspaceName?: string
    branch?: string
  }
}

interface ResumeSuspendedTabPayload extends TabCommandPayloadBase {
  tabId: string
  options?: {
    workspaceName?: string
    branch?: string
  }
}

interface SaveCurrentLayoutPayload {
  worktreePath: string
}

interface FocusSessionPayload {
  sessionId: string
}

interface ClosedTabEntry {
  toolId: string
  toolName: string
  profileId?: string
}

type EditorWriteFileResult =
  | { ok: true; mtimeMs: number; size: number }
  | { ok: false; tag: 'StaleWrite'; actualMtimeMs: number }
  | { ok: false; tag: 'WriteFailed' | 'StatFailed'; message: string }

interface DirtyEditorFile {
  paneId: string
  filePath: string
  currentContent?: string
  fileLineEnding?: 'LF' | 'CRLF'
  fileMtimeMs?: number
}

function allPaneSnapshots(split: SplitSnapshot): PaneSnapshot[] {
  validateSplitSnapshot(split)
  if (split.type === 'leaf') return [split.pane]
  return [...allPaneSnapshots(split.first), ...allPaneSnapshots(split.second)]
}

function firstPaneSnapshot(split: SplitSnapshot): PaneSnapshot {
  if (split.type === 'leaf') return split.pane
  return firstPaneSnapshot(split.first)
}

function splitDepth(split: SplitSnapshot): number {
  if (split.type === 'leaf') return 1
  return 1 + Math.max(splitDepth(split.first), splitDepth(split.second))
}

const MAX_SPLIT_DEPTH = 4
const MAX_CLOSED_TABS = 20
const AI_TOOL_IDS = new Set(['claude', 'codex', 'opencode', 'gemini'])
const NO_SPLIT_TOOLS = new Set(['claude', 'codex', 'opencode', 'gemini'])

function splitPaneSnapshot(
  split: SplitSnapshot,
  paneId: string,
  direction: 'horizontal' | 'vertical',
  pane: PaneSnapshot,
): SplitSnapshot | null {
  if (splitDepth(split) >= MAX_SPLIT_DEPTH) return null
  return splitPaneSnapshotInner(split, paneId, direction, pane)
}

function splitPaneSnapshotInner(
  split: SplitSnapshot,
  paneId: string,
  direction: 'horizontal' | 'vertical',
  pane: PaneSnapshot,
): SplitSnapshot | null {
  if (split.type === 'leaf') {
    if (split.pane.id !== paneId) return null
    return {
      type: 'split',
      id: splitId(),
      direction,
      ratio: 0.5,
      first: split,
      second: { type: 'leaf', pane },
    }
  }

  const first = splitPaneSnapshotInner(split.first, paneId, direction, pane)
  if (first) return { ...split, first }

  const second = splitPaneSnapshotInner(split.second, paneId, direction, pane)
  if (second) return { ...split, second }

  return null
}

function removePaneSnapshot(
  split: SplitSnapshot,
  paneId: string,
): { tree: SplitSnapshot | null; removed: PaneSnapshot } | null {
  if (split.type === 'leaf') {
    if (split.pane.id !== paneId) return null
    return { tree: null, removed: split.pane }
  }

  const first = removePaneSnapshot(split.first, paneId)
  if (first) {
    return {
      tree: first.tree ? { ...split, first: first.tree } : split.second,
      removed: first.removed,
    }
  }

  const second = removePaneSnapshot(split.second, paneId)
  if (second) {
    return {
      tree: second.tree ? { ...split, second: second.tree } : split.first,
      removed: second.removed,
    }
  }

  return null
}

function graftSplitSnapshot(
  split: SplitSnapshot,
  targetPaneId: string,
  direction: 'horizontal' | 'vertical',
  subtree: SplitSnapshot,
  position: 'first' | 'second',
): SplitSnapshot | null {
  return graftSplitSnapshotInner(
    split,
    targetPaneId,
    direction,
    subtree,
    position,
    splitDepth(subtree),
    1,
  )
}

function graftSplitSnapshotInner(
  split: SplitSnapshot,
  targetPaneId: string,
  direction: 'horizontal' | 'vertical',
  subtree: SplitSnapshot,
  position: 'first' | 'second',
  subtreeDepth: number,
  currentDepth: number,
): SplitSnapshot | null {
  if (split.type === 'leaf') {
    if (split.pane.id !== targetPaneId) return null
    if (currentDepth + subtreeDepth > MAX_SPLIT_DEPTH) return null

    return {
      type: 'split',
      id: splitId(),
      direction,
      ratio: 0.5,
      first: position === 'first' ? subtree : split,
      second: position === 'first' ? split : subtree,
    }
  }

  const first = graftSplitSnapshotInner(
    split.first,
    targetPaneId,
    direction,
    subtree,
    position,
    subtreeDepth,
    currentDepth + 1,
  )
  if (first) return { ...split, first }

  const second = graftSplitSnapshotInner(
    split.second,
    targetPaneId,
    direction,
    subtree,
    position,
    subtreeDepth,
    currentDepth + 1,
  )
  if (second) return { ...split, second }

  return null
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

function editorFileList(
  list: EditorFileSnapshot[] | undefined,
  filePath: string,
  legacySingle: string | undefined,
): EditorFileSnapshot[] {
  const base = list ?? (legacySingle ? [{ filePath: legacySingle }] : [])
  if (base.some((file) => file.filePath === filePath)) return base
  return [...base, { filePath }]
}

function paneHasEditorFile(pane: PaneSnapshot, filePath: string): boolean {
  return (
    pane.paneType === 'editor' &&
    (pane.filePath === filePath ||
      (pane.editorFiles ?? []).some((file) => file.filePath === filePath))
  )
}

function paneWithEditorFile(pane: PaneSnapshot, filePath: string): PaneSnapshot {
  return {
    ...pane,
    filePath,
    editorActiveFile: filePath,
    editorFiles: editorFileList(pane.editorFiles, filePath, pane.filePath),
  }
}

function paneWithoutEditorFile(pane: PaneSnapshot, filePath: string): PaneSnapshot | null {
  const files = pane.editorFiles ?? (pane.filePath ? [{ filePath: pane.filePath }] : [])
  if (!files.some((file) => file.filePath === filePath)) return pane

  const remaining = files.filter((file) => file.filePath !== filePath)
  if (remaining.length === 0) return null

  const activeFile =
    pane.editorActiveFile === filePath
      ? remaining[Math.max(0, remaining.length - 1)].filePath
      : (pane.editorActiveFile ?? remaining[0].filePath)

  return {
    ...pane,
    editorFiles: remaining,
    editorActiveFile: activeFile,
    filePath: activeFile,
  }
}

function paneWithMovedEditorFile(
  pane: PaneSnapshot,
  filePath: string,
  toIndex: number,
): PaneSnapshot | null {
  const files = pane.editorFiles ?? (pane.filePath ? [{ filePath: pane.filePath }] : [])
  const fromIndex = files.findIndex((file) => file.filePath === filePath)
  if (fromIndex < 0) return null

  const clamped = Math.max(0, Math.min(toIndex, files.length))
  if (clamped === fromIndex || clamped === fromIndex + 1) return pane

  const next = [...files]
  const [moved] = next.splice(fromIndex, 1)
  const insertAt = clamped > fromIndex ? clamped - 1 : clamped
  next.splice(insertAt, 0, moved)

  return {
    ...pane,
    editorFiles: next,
  }
}

function paneWithActiveEditorFile(pane: PaneSnapshot, filePath: string): PaneSnapshot | null {
  const files = pane.editorFiles ?? (pane.filePath ? [{ filePath: pane.filePath }] : [])
  if (!files.some((file) => file.filePath === filePath)) return null

  return {
    ...pane,
    editorActiveFile: filePath,
    filePath,
  }
}

function paneWithUpdatedEditorFileState(
  pane: PaneSnapshot,
  filePath: string,
  patch: Partial<EditorFileSnapshot>,
): PaneSnapshot | null {
  const files = pane.editorFiles ?? (pane.filePath ? [{ filePath: pane.filePath }] : [])
  if (!files.some((file) => file.filePath === filePath)) return null

  return {
    ...pane,
    editorFiles: files.map((file) => (file.filePath === filePath ? { ...file, ...patch } : file)),
  }
}

function paneWithInsertedEditorFile(
  pane: PaneSnapshot,
  file: EditorFileSnapshot,
  toIndex: number,
): PaneSnapshot {
  const files = pane.editorFiles ?? (pane.filePath ? [{ filePath: pane.filePath }] : [])
  if (files.some((candidate) => candidate.filePath === file.filePath)) {
    return {
      ...pane,
      editorActiveFile: file.filePath,
      filePath: file.filePath,
    }
  }

  const clamped = Math.max(0, Math.min(toIndex, files.length))
  const next = [...files]
  next.splice(clamped, 0, file)

  return {
    ...pane,
    editorFiles: next,
    editorActiveFile: file.filePath,
    filePath: file.filePath,
  }
}

function updateSplitRatioSnapshot(
  split: SplitSnapshot,
  splitId: string,
  ratio: number,
): { split: SplitSnapshot; changed: boolean } {
  if (split.type === 'leaf') return { split, changed: false }
  if (split.id === splitId) {
    return { split: { ...split, ratio }, changed: true }
  }

  const first = updateSplitRatioSnapshot(split.first, splitId, ratio)
  if (first.changed) {
    return { split: { ...split, first: first.split }, changed: true }
  }

  const second = updateSplitRatioSnapshot(split.second, splitId, ratio)
  if (second.changed) {
    return { split: { ...split, second: second.split }, changed: true }
  }

  return { split, changed: false }
}

interface LeafRectSnapshot {
  paneId: string
  x: number
  y: number
  w: number
  h: number
}

function buildLeafRectSnapshots(
  split: SplitSnapshot,
  x: number,
  y: number,
  w: number,
  h: number,
): LeafRectSnapshot[] {
  if (split.type === 'leaf') return [{ paneId: split.pane.id, x, y, w, h }]

  if (split.direction === 'vertical') {
    const firstW = w * split.ratio
    const secondW = w - firstW
    return [
      ...buildLeafRectSnapshots(split.first, x, y, firstW, h),
      ...buildLeafRectSnapshots(split.second, x + firstW, y, secondW, h),
    ]
  }

  const firstH = h * split.ratio
  const secondH = h - firstH
  return [
    ...buildLeafRectSnapshots(split.first, x, y, w, firstH),
    ...buildLeafRectSnapshots(split.second, x, y + firstH, w, secondH),
  ]
}

function navigatePaneSnapshot(
  split: SplitSnapshot,
  fromPaneId: string,
  direction: 'left' | 'right' | 'up' | 'down',
): string | null {
  const rects = buildLeafRectSnapshots(split, 0, 0, 1, 1)
  const source = rects.find((rect) => rect.paneId === fromPaneId)
  if (!source) return null

  const sourceCenterX = source.x + source.w / 2
  const sourceCenterY = source.y + source.h / 2
  const eps = 0.001
  const candidates = rects.filter((rect) => {
    if (rect.paneId === fromPaneId) return false
    const centerX = rect.x + rect.w / 2
    const centerY = rect.y + rect.h / 2
    if (direction === 'right') return centerX > sourceCenterX + eps
    if (direction === 'left') return centerX < sourceCenterX - eps
    if (direction === 'down') return centerY > sourceCenterY + eps
    return centerY < sourceCenterY - eps
  })

  if (candidates.length === 0) return null

  candidates.sort((a, b) => {
    const aCenterX = a.x + a.w / 2
    const aCenterY = a.y + a.h / 2
    const bCenterX = b.x + b.w / 2
    const bCenterY = b.y + b.h / 2
    const distanceA = Math.abs(aCenterX - sourceCenterX) + Math.abs(aCenterY - sourceCenterY)
    const distanceB = Math.abs(bCenterX - sourceCenterX) + Math.abs(bCenterY - sourceCenterY)
    return distanceA - distanceB
  })

  return candidates[0].paneId
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

function splitId(): string {
  return `split-${randomUUID()}`
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

function assertOptionalBoolean(
  value: unknown,
  label: string,
): asserts value is boolean | undefined {
  if (value !== undefined && typeof value !== 'boolean') {
    throw new Error(`Invalid tab snapshot: ${label} must be a boolean`)
  }
}

function assertOptionalNumber(value: unknown, label: string): asserts value is number | undefined {
  if (value !== undefined && typeof value !== 'number') {
    throw new Error(`Invalid tab snapshot: ${label} must be a number`)
  }
}

function assertOptionalLineEnding(
  value: unknown,
  label: string,
): asserts value is 'LF' | 'CRLF' | undefined {
  if (value !== undefined && value !== 'LF' && value !== 'CRLF') {
    throw new Error(`Invalid tab snapshot: ${label} must be LF or CRLF`)
  }
}

function assertOptionalStringArray(
  value: unknown,
  label: string,
): asserts value is string[] | undefined {
  if (value === undefined) return
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string')) {
    throw new Error(`Invalid tab snapshot: ${label} must be an array of strings`)
  }
}

function validateEditorFileSnapshot(value: unknown): asserts value is EditorFileSnapshot {
  if (!isRecord(value)) throw new Error('Invalid tab snapshot: editor file must be an object')
  assertString(value.filePath, 'editorFile.filePath')
  assertOptionalBoolean(value.dirty, 'editorFile.dirty')
  assertOptionalString(value.originalContent, 'editorFile.originalContent')
  assertOptionalString(value.currentContent, 'editorFile.currentContent')
  assertOptionalNumber(value.fileMtimeMs, 'editorFile.fileMtimeMs')
  assertOptionalLineEnding(value.fileLineEnding, 'editorFile.fileLineEnding')
  assertOptionalBoolean(value.externalChangeDetected, 'editorFile.externalChangeDetected')
}

function editorFileStatePatch(value: unknown): Partial<EditorFileSnapshot> {
  if (!isRecord(value)) throw new Error('Invalid tab command: editor file patch must be an object')

  const patch: Partial<EditorFileSnapshot> = {}
  if ('dirty' in value) {
    assertOptionalBoolean(value.dirty, 'editorFile.dirty')
    patch.dirty = value.dirty
  }
  if ('originalContent' in value) {
    assertOptionalString(value.originalContent, 'editorFile.originalContent')
    patch.originalContent = value.originalContent
  }
  if ('currentContent' in value) {
    assertOptionalString(value.currentContent, 'editorFile.currentContent')
    patch.currentContent = value.currentContent
  }
  if ('fileMtimeMs' in value) {
    assertOptionalNumber(value.fileMtimeMs, 'editorFile.fileMtimeMs')
    patch.fileMtimeMs = value.fileMtimeMs
  }
  if ('fileLineEnding' in value) {
    assertOptionalLineEnding(value.fileLineEnding, 'editorFile.fileLineEnding')
    patch.fileLineEnding = value.fileLineEnding
  }
  if ('externalChangeDetected' in value) {
    assertOptionalBoolean(value.externalChangeDetected, 'editorFile.externalChangeDetected')
    patch.externalChangeDetected = value.externalChangeDetected
  }

  return patch
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
  if (value.editorFiles !== undefined) {
    if (!Array.isArray(value.editorFiles)) {
      throw new Error('Invalid tab snapshot: pane.editorFiles must be an array')
    }
    value.editorFiles.forEach(validateEditorFileSnapshot)
  }
  if (value.detached !== undefined && typeof value.detached !== 'boolean') {
    throw new Error('Invalid tab snapshot: pane.detached must be a boolean')
  }
  if (value.inspectorOpen !== undefined && typeof value.inspectorOpen !== 'boolean') {
    throw new Error('Invalid tab snapshot: pane.inspectorOpen must be a boolean')
  }
}

function validateSerializedSplitNode(value: unknown): asserts value is SerializedSplitNode {
  if (!isRecord(value)) throw new Error('Invalid serialized layout: split node must be an object')

  if (value.type === 'leaf') {
    assertString(value.toolId, 'serializedLeaf.toolId')
    assertString(value.toolName, 'serializedLeaf.toolName')
    assertOptionalString(value.agentSessionId, 'serializedLeaf.agentSessionId')
    assertOptionalString(value.claudeSessionId, 'serializedLeaf.claudeSessionId')
    assertOptionalString(value.browserUrl, 'serializedLeaf.browserUrl')
    if (
      value.browserDevToolsMode !== undefined &&
      value.browserDevToolsMode !== 'bottom' &&
      value.browserDevToolsMode !== 'right'
    ) {
      throw new Error('Invalid serialized layout: browserDevToolsMode is invalid')
    }
    assertOptionalString(value.filePath, 'serializedLeaf.filePath')
    assertOptionalStringArray(value.editorFiles, 'serializedLeaf.editorFiles')
    assertOptionalString(value.editorActiveFile, 'serializedLeaf.editorActiveFile')
    assertOptionalString(value.tmuxSessionName, 'serializedLeaf.tmuxSessionName')
    assertOptionalString(value.profileId, 'serializedLeaf.profileId')
    return
  }

  if (value.type !== 'hsplit' && value.type !== 'vsplit') {
    throw new Error('Invalid serialized layout: split node type is invalid')
  }
  if (typeof value.ratio !== 'number' || !Number.isFinite(value.ratio)) {
    throw new Error('Invalid serialized layout: split ratio must be a finite number')
  }
  validateSerializedSplitNode(value.first)
  validateSerializedSplitNode(value.second)
}

function cloneSerializedSplitNode(node: SerializedSplitNode): SerializedSplitNode {
  if (node.type === 'leaf') {
    return {
      type: 'leaf',
      toolId: node.toolId,
      toolName: node.toolName,
      agentSessionId: node.agentSessionId,
      claudeSessionId: node.claudeSessionId,
      browserUrl: node.browserUrl,
      browserDevToolsMode: node.browserDevToolsMode,
      filePath: node.filePath,
      editorFiles: node.editorFiles ? [...node.editorFiles] : undefined,
      editorActiveFile: node.editorActiveFile,
      tmuxSessionName: node.tmuxSessionName,
      profileId: node.profileId,
    }
  }
  return {
    type: node.type,
    first: cloneSerializedSplitNode(node.first),
    second: cloneSerializedSplitNode(node.second),
    ratio: node.ratio,
  }
}

function validateSerializedLayout(value: unknown): asserts value is SerializedLayout {
  if (!isRecord(value)) throw new Error('Invalid serialized layout: layout must be an object')
  if (!Array.isArray(value.tabs)) {
    throw new Error('Invalid serialized layout: tabs must be an array')
  }
  if (typeof value.activeTabIndex !== 'number' || !Number.isFinite(value.activeTabIndex)) {
    throw new Error('Invalid serialized layout: activeTabIndex must be a finite number')
  }
  for (const tab of value.tabs) {
    if (!isRecord(tab)) throw new Error('Invalid serialized layout: tab must be an object')
    assertString(tab.toolId, 'serializedTab.toolId')
    assertString(tab.toolName, 'serializedTab.toolName')
    validateSerializedSplitNode(tab.rootSplit)
  }
}

function validateSplitSnapshot(value: unknown): asserts value is SplitSnapshot {
  if (!isRecord(value)) throw new Error('Invalid tab snapshot: split must be an object')
  if (value.type === 'leaf') {
    validatePaneSnapshot(value.pane)
    return
  }
  if (value.type !== 'split') throw new Error('Invalid tab snapshot: split.type is invalid')
  assertString(value.id, 'split.id')
  if (value.direction !== 'horizontal' && value.direction !== 'vertical') {
    throw new Error('Invalid tab snapshot: split.direction is invalid')
  }
  if (typeof value.ratio !== 'number' || !Number.isFinite(value.ratio)) {
    throw new Error('Invalid tab snapshot: split.ratio must be a finite number')
  }
  validateSplitSnapshot(value.first)
  validateSplitSnapshot(value.second)
}

function emptyResult(
  worktreePath: string,
  tabs: TabSnapshot[],
  activeTabId: string | null,
): TabCommandResult {
  return { worktreePath, tabs, activeTabId }
}

function getTabDisplayName(tab: TabSnapshot): string {
  const focused = allPaneSnapshots(tab.rootSplit).find((pane) => pane.id === tab.focusedPaneId)
  return focused?.title || tab.name
}

function reconcileTabSnapshotIdentity(tab: TabSnapshot, tabs: TabSnapshot[]): TabSnapshot {
  const focused = allPaneSnapshots(tab.rootSplit).find((pane) => pane.id === tab.focusedPaneId)
  if (!focused || tab.toolId === focused.toolId) return tab

  const sameCount = tabs.filter(
    (candidate) => candidate.id !== tab.id && candidate.toolId === focused.toolId,
  ).length
  return {
    ...tab,
    toolId: focused.toolId,
    toolName: focused.toolName,
    name: sameCount === 0 ? focused.toolName : `${focused.toolName} #${sameCount + 1}`,
  }
}

export class TabCommandService {
  private tabsByWorktreeByWindow = new Map<number, Map<string, TabSnapshot[]>>()
  private activeTabIdByWorktreeByWindow = new Map<number, Map<string, string | null>>()
  private closedTabsByWorktreeByWindow = new Map<number, Map<string, ClosedTabEntry[]>>()
  private trackedWebContents = new Set<number>()

  constructor(private deps: TabCommandServiceDeps) {}

  async openTool(sender: WebContents, payload: OpenToolPayload): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
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
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: id,
      openedTab,
    }
    this.setTabState(sender, payload.worktreePath, tabs, id)
    return result
  }

  openDiffTab(sender: WebContents, payload: OpenDiffPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)

    for (const [tabIndex, tab] of tabs.entries()) {
      const diffPane = allPaneSnapshots(tab.rootSplit).find((pane) => pane.paneType === 'diff')
      if (!diffPane) continue

      tabs[tabIndex] = { ...tab, focusedPaneId: diffPane.id }
      const result = {
        worktreePath: payload.worktreePath,
        tabs,
        activeTabId: tab.id,
      }
      this.setTabState(sender, payload.worktreePath, tabs, tab.id)
      return result
    }

    const pane: PaneSnapshot = {
      id: paneId(),
      sessionId: '',
      wsUrl: '',
      toolId: 'diff',
      toolName: 'Diff',
      isRunning: false,
      exitCode: null,
      title: null,
      paneType: 'diff',
    }
    const id = tabId()
    const openedTab: TabSnapshot = {
      id,
      toolId: 'diff',
      toolName: 'Diff',
      name: computeDisplayName('Diff', payload.worktreePath, 'diff', tabs),
      worktreePath: payload.worktreePath,
      rootSplit: { type: 'leaf', pane },
      focusedPaneId: pane.id,
    }

    tabs.push(openedTab)
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: id,
      openedTab,
    }
    this.setTabState(sender, payload.worktreePath, tabs, id)
    return result
  }

  openSessionTab(sender: WebContents, payload: OpenSessionTabPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const session = this.deps.toolSessions.getOwnedSessionConnection(sender, payload.sessionId)
    const tabs = this.getCommandTabs(sender.id, payload)
    const pane: PaneSnapshot = {
      id: paneId(),
      sessionId: payload.sessionId,
      wsUrl: session.wsUrl,
      toolId: 'shell',
      toolName: 'Shell',
      isRunning: true,
      exitCode: null,
      title: null,
      tmuxSessionName: session.tmuxSessionName,
    }
    const id = tabId()
    const openedTab: TabSnapshot = {
      id,
      toolId: 'shell',
      toolName: payload.name,
      name: computeDisplayName(payload.name, payload.worktreePath, 'shell', tabs),
      worktreePath: payload.worktreePath,
      rootSplit: { type: 'leaf', pane },
      focusedPaneId: pane.id,
    }

    tabs.push(openedTab)
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: id,
      openedTab,
    }
    this.setTabState(sender, payload.worktreePath, tabs, id)
    return result
  }

  openEditorFile(sender: WebContents, payload: OpenEditorFilePayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)

    for (const [tabIndex, tab] of tabs.entries()) {
      if (tab.suspended) continue
      const existing = allPaneSnapshots(tab.rootSplit).find((pane) =>
        paneHasEditorFile(pane, payload.filePath),
      )
      if (!existing) continue

      tabs[tabIndex] = {
        ...tab,
        rootSplit: updatePaneSnapshot(tab.rootSplit, existing.id, (pane) =>
          paneWithEditorFile(pane, payload.filePath),
        ),
        focusedPaneId: existing.id,
      }
      const result = {
        worktreePath: payload.worktreePath,
        tabs,
        activeTabId: tab.id,
      }
      this.setTabState(sender, payload.worktreePath, tabs, tab.id)
      return result
    }

    const activeTabIndex = tabs.findIndex((tab) => tab.id === activeId && !tab.suspended)
    if (activeTabIndex >= 0) {
      const activeTab = tabs[activeTabIndex]
      const panes = allPaneSnapshots(activeTab.rootSplit)
      const editorPane =
        panes.find((pane) => pane.id === activeTab.focusedPaneId && pane.paneType === 'editor') ??
        panes.find((pane) => pane.paneType === 'editor')

      if (editorPane) {
        tabs[activeTabIndex] = {
          ...activeTab,
          rootSplit: updatePaneSnapshot(activeTab.rootSplit, editorPane.id, (pane) =>
            paneWithEditorFile(pane, payload.filePath),
          ),
          focusedPaneId: editorPane.id,
        }
        const result = {
          worktreePath: payload.worktreePath,
          tabs,
          activeTabId: activeTab.id,
        }
        this.setTabState(sender, payload.worktreePath, tabs, activeTab.id)
        return result
      }
    }

    const pane: PaneSnapshot = {
      id: paneId(),
      sessionId: '',
      wsUrl: '',
      toolId: 'editor',
      toolName: 'Editor',
      isRunning: true,
      exitCode: null,
      title: null,
      paneType: 'editor',
      filePath: payload.filePath,
      editorFiles: [{ filePath: payload.filePath }],
      editorActiveFile: payload.filePath,
    }
    const id = tabId()
    const openedTab: TabSnapshot = {
      id,
      toolId: 'editor',
      toolName: 'Editor',
      name: computeDisplayName('Editor', payload.worktreePath, 'editor', tabs),
      worktreePath: payload.worktreePath,
      rootSplit: { type: 'leaf', pane },
      focusedPaneId: pane.id,
    }

    tabs.push(openedTab)
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: id,
      openedTab,
    }
    this.setTabState(sender, payload.worktreePath, tabs, id)
    return result
  }

  detachEditorFile(sender: WebContents, payload: DetachEditorFilePayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    const detachedFile = (
      pane.editorFiles ?? (pane.filePath ? [{ filePath: pane.filePath }] : [])
    ).find((file) => file.filePath === payload.filePath) ?? { filePath: payload.filePath }
    const updatedPane = paneWithoutEditorFile(pane, payload.filePath)
    if (!updatedPane) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, () => updatedPane),
    }

    const detachedPane: PaneSnapshot = {
      id: paneId(),
      sessionId: '',
      wsUrl: '',
      toolId: 'editor',
      toolName: 'Editor',
      isRunning: true,
      exitCode: null,
      title: null,
      paneType: 'editor',
      filePath: payload.filePath,
      editorFiles: [detachedFile],
      editorActiveFile: payload.filePath,
    }
    const openedTab: TabSnapshot = {
      id: tabId(),
      toolId: 'editor',
      toolName: 'Editor',
      name: computeDisplayName('Editor', payload.worktreePath, 'editor', tabs),
      worktreePath: payload.worktreePath,
      rootSplit: { type: 'leaf', pane: detachedPane },
      focusedPaneId: detachedPane.id,
    }
    tabs.push(openedTab)

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: openedTab.id,
      openedTab,
    }
    this.setTabState(sender, payload.worktreePath, tabs, openedTab.id)
    return result
  }

  closeEditorFile(sender: WebContents, payload: CloseEditorFilePayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    const updatedPane = paneWithoutEditorFile(pane, payload.filePath)
    if (updatedPane) {
      tabs[tabIndex] = {
        ...tab,
        rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, () => updatedPane),
      }
      const result = {
        worktreePath: payload.worktreePath,
        tabs,
        activeTabId: activeId ?? tab.id,
        closedPaneId: payload.paneId,
      }
      this.setTabState(sender, payload.worktreePath, tabs, result.activeTabId)
      return result
    }

    const removeResult = removePaneSnapshot(tab.rootSplit, payload.paneId)
    if (!removeResult) return emptyResult(payload.worktreePath, tabs, activeId)

    let nextActiveId = activeId
    let closedTabId: string | undefined
    if (!removeResult.tree) {
      tabs.splice(tabIndex, 1)
      closedTabId = tab.id
      if (nextActiveId === tab.id) {
        nextActiveId = tabs.length > 0 ? tabs[Math.min(tabIndex, tabs.length - 1)].id : null
      }
    } else {
      tabs[tabIndex] = {
        ...tab,
        rootSplit: removeResult.tree,
        focusedPaneId:
          tab.focusedPaneId === payload.paneId
            ? firstPaneSnapshot(removeResult.tree).id
            : tab.focusedPaneId,
      }
      nextActiveId = nextActiveId ?? tab.id
    }

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: nextActiveId,
      closedTabId,
      closedPaneId: payload.paneId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, nextActiveId)
    return result
  }

  moveEditorFile(sender: WebContents, payload: MoveEditorFilePayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    const updatedPane = paneWithMovedEditorFile(pane, payload.filePath, payload.toIndex)
    if (!updatedPane || updatedPane === pane)
      return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, () => updatedPane),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId ?? tab.id,
    }
    this.setTabState(sender, payload.worktreePath, tabs, result.activeTabId)
    return result
  }

  moveEditorFileBetweenPanes(
    sender: WebContents,
    payload: MoveEditorFileBetweenPanesPayload,
  ): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)

    if (payload.sourcePaneId === payload.targetPaneId) {
      return this.moveEditorFile(sender, {
        worktreePath: payload.worktreePath,
        paneId: payload.targetPaneId,
        filePath: payload.filePath,
        toIndex: payload.toIndex,
      })
    }

    const sourceTabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.sourcePaneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    const targetTabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some((pane) => pane.id === payload.targetPaneId),
    )
    if (sourceTabIndex < 0 || targetTabIndex < 0) {
      return emptyResult(payload.worktreePath, tabs, activeId)
    }

    const sourceTab = tabs[sourceTabIndex]
    const targetTab = tabs[targetTabIndex]
    const sourcePane = allPaneSnapshots(sourceTab.rootSplit).find(
      (candidate) => candidate.id === payload.sourcePaneId,
    )
    const targetPane = allPaneSnapshots(targetTab.rootSplit).find(
      (candidate) => candidate.id === payload.targetPaneId,
    )
    if (!sourcePane || !targetPane) return emptyResult(payload.worktreePath, tabs, activeId)

    const sourceFiles =
      sourcePane.editorFiles ?? (sourcePane.filePath ? [{ filePath: sourcePane.filePath }] : [])
    const movingFile = sourceFiles.find((file) => file.filePath === payload.filePath)
    if (!movingFile) return emptyResult(payload.worktreePath, tabs, activeId)

    let nextActiveId = activeId
    const targetTabId = targetTab.id
    const updatedSourcePane = paneWithoutEditorFile(sourcePane, payload.filePath)
    if (!updatedSourcePane) {
      const removeResult = removePaneSnapshot(sourceTab.rootSplit, payload.sourcePaneId)
      if (!removeResult) return emptyResult(payload.worktreePath, tabs, activeId)

      if (!removeResult.tree) {
        tabs.splice(sourceTabIndex, 1)
        if (nextActiveId === sourceTab.id) nextActiveId = targetTabId
      } else {
        tabs[sourceTabIndex] = {
          ...sourceTab,
          rootSplit: removeResult.tree,
          focusedPaneId:
            sourceTab.focusedPaneId === payload.sourcePaneId
              ? firstPaneSnapshot(removeResult.tree).id
              : sourceTab.focusedPaneId,
        }
      }
    } else {
      tabs[sourceTabIndex] = {
        ...sourceTab,
        rootSplit: updatePaneSnapshot(
          sourceTab.rootSplit,
          payload.sourcePaneId,
          () => updatedSourcePane,
        ),
      }
    }

    const currentTargetTabIndex = tabs.findIndex((tab) => tab.id === targetTabId)
    if (currentTargetTabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const currentTargetTab = tabs[currentTargetTabIndex]
    const currentTargetPane = allPaneSnapshots(currentTargetTab.rootSplit).find(
      (candidate) => candidate.id === payload.targetPaneId,
    )
    if (!currentTargetPane) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[currentTargetTabIndex] = {
      ...currentTargetTab,
      rootSplit: updatePaneSnapshot(currentTargetTab.rootSplit, payload.targetPaneId, (pane) =>
        paneWithInsertedEditorFile(pane, movingFile, payload.toIndex),
      ),
      focusedPaneId: payload.targetPaneId,
    }
    nextActiveId = targetTabId

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: nextActiveId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, nextActiveId)
    return result
  }

  setActiveEditorFile(sender: WebContents, payload: SetActiveEditorFilePayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    const updatedPane = paneWithActiveEditorFile(pane, payload.filePath)
    if (!updatedPane) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, () => updatedPane),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  updateEditorFileState(
    sender: WebContents,
    payload: UpdateEditorFileStatePayload,
  ): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    const patch = editorFileStatePatch(payload.patch)
    const updatedPane = paneWithUpdatedEditorFileState(pane, payload.filePath, patch)
    if (!updatedPane) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, () => updatedPane),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  async loadEditorFile(
    sender: WebContents,
    payload: LoadEditorFilePayload,
  ): Promise<EditorFileLoadResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (tabIndex < 0) {
      return { ok: false, tag: 'ReadFailed', message: 'Editor file is not open' }
    }

    const loadResult = await this.deps.loadEditorFile(
      sender,
      payload.filePath,
      payload.options?.maxBytes,
    )
    if (!loadResult.ok) return loadResult

    const patch: Partial<EditorFileSnapshot> = loadResult.binary
      ? {
          dirty: false,
          originalContent: undefined,
          currentContent: undefined,
          fileMtimeMs: loadResult.mtimeMs,
          fileLineEnding: undefined,
          externalChangeDetected: false,
        }
      : {
          dirty: false,
          originalContent: loadResult.content,
          currentContent: loadResult.content,
          fileMtimeMs: loadResult.mtimeMs,
          fileLineEnding: loadResult.fileLineEnding,
          externalChangeDetected: false,
        }

    const currentTabs = this.getCommandTabs(sender.id, payload)
    const currentActiveId = this.getCommandActiveTabId(sender.id, payload)
    const currentTabIndex = currentTabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (currentTabIndex < 0) {
      return { ok: false, tag: 'ReadFailed', message: 'Editor file is not open' }
    }

    const currentTab = currentTabs[currentTabIndex]
    const currentPane = allPaneSnapshots(currentTab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!currentPane) return { ok: false, tag: 'ReadFailed', message: 'Editor pane is not open' }

    const updatedPane = paneWithUpdatedEditorFileState(currentPane, payload.filePath, patch)
    if (!updatedPane) {
      return { ok: false, tag: 'ReadFailed', message: 'Editor file is not open' }
    }

    currentTabs[currentTabIndex] = {
      ...currentTab,
      rootSplit: updatePaneSnapshot(currentTab.rootSplit, payload.paneId, () => updatedPane),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs: currentTabs,
      activeTabId: currentActiveId,
    }
    this.setTabState(sender, payload.worktreePath, currentTabs, currentActiveId)
    return { ...loadResult, result }
  }

  async saveEditorFile(
    sender: WebContents,
    payload: SaveEditorFilePayload,
  ): Promise<EditorFileSaveResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (tabIndex < 0) {
      return { ok: false, tag: 'WriteFailed', message: 'Editor file is not open' }
    }

    const content = payload.options.content
    const normalized =
      payload.options.fileLineEnding === 'CRLF' ? content.replace(/\r?\n/g, '\r\n') : content
    const writeResult = await this.deps.writeEditorFile(
      sender,
      payload.filePath,
      normalized,
      payload.options.expectedMtimeMs,
    )
    if (!writeResult.ok) return writeResult

    const currentTabs = this.getCommandTabs(sender.id, payload)
    const currentActiveId = this.getCommandActiveTabId(sender.id, payload)
    const currentTabIndex = currentTabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some(
        (pane) => pane.id === payload.paneId && paneHasEditorFile(pane, payload.filePath),
      ),
    )
    if (currentTabIndex < 0) {
      return { ok: false, tag: 'WriteFailed', message: 'Editor file is not open' }
    }

    const currentTab = currentTabs[currentTabIndex]
    const currentPane = allPaneSnapshots(currentTab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!currentPane) return { ok: false, tag: 'WriteFailed', message: 'Editor pane is not open' }

    const updatedPane = paneWithUpdatedEditorFileState(currentPane, payload.filePath, {
      dirty: false,
      originalContent: content,
      currentContent: content,
      fileMtimeMs: writeResult.mtimeMs,
      externalChangeDetected: false,
    })
    if (!updatedPane) {
      return { ok: false, tag: 'WriteFailed', message: 'Editor file is not open' }
    }

    currentTabs[currentTabIndex] = {
      ...currentTab,
      rootSplit: updatePaneSnapshot(currentTab.rootSplit, payload.paneId, () => updatedPane),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs: currentTabs,
      activeTabId: currentActiveId,
    }
    this.setTabState(sender, payload.worktreePath, currentTabs, currentActiveId)
    return { ok: true, mtimeMs: writeResult.mtimeMs, size: writeResult.size, result }
  }

  async prepareCloseEditorFile(
    sender: WebContents,
    payload: PrepareCloseEditorFilePayload,
  ): Promise<TabClosePreflightResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const file = this.editorFileSnapshot(tabs, payload.paneId, payload.filePath)
    if (!file?.dirty) return { ok: true }

    const choice = await this.deps.confirmUnsavedChanges(sender, [payload.filePath])
    if (choice === 'cancel') return { ok: false, reason: 'cancelled' }
    if (choice === 'discard') return { ok: true }

    const saveResult = await this.saveEditorFile(sender, {
      worktreePath: payload.worktreePath,
      paneId: payload.paneId,
      filePath: payload.filePath,
      options: {
        content: file.currentContent ?? '',
        fileLineEnding: file.fileLineEnding,
        expectedMtimeMs: file.fileMtimeMs,
      },
    })
    if (!saveResult.ok) return { ok: false, reason: 'save-failed', failedCount: 1 }

    return { ok: true }
  }

  updatePaneTitle(sender: WebContents, payload: UpdatePaneTitlePayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some((pane) => pane.sessionId === payload.sessionId),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.sessionId === payload.sessionId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, pane.id, (previous) => ({
        ...previous,
        title: payload.title,
      })),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  updatePaneUrl(sender: WebContents, payload: UpdatePaneUrlPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some((pane) => pane.sessionId === payload.sessionId),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.sessionId === payload.sessionId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, pane.id, (previous) => ({
        ...previous,
        url: payload.url,
      })),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  updateTmuxSessionName(
    sender: WebContents,
    payload: UpdateTmuxSessionNamePayload,
  ): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some((pane) => pane.tmuxSessionName === payload.oldName),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.tmuxSessionName === payload.oldName,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, pane.id, (previous) => ({
        ...previous,
        tmuxSessionName: payload.newName,
      })),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  async handlePtyExit(
    sender: WebContents,
    payload: HandlePtyExitPayload,
  ): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) =>
      allPaneSnapshots(tab.rootSplit).some((pane) => pane.sessionId === payload.sessionId),
    )
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.sessionId === payload.sessionId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    const tmuxName = payload.tmuxSessionName || pane.tmuxSessionName
    const detached = tmuxName
      ? await this.deps.toolSessions.hasTmuxSession(tmuxName).catch(() => false)
      : false

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, pane.id, (previous) => ({
        ...previous,
        isRunning: false,
        exitCode: payload.exitCode,
        detached,
        tmuxSessionName: tmuxName,
      })),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  async killTmuxPane(sender: WebContents, payload: KillTmuxPanePayload): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane?.tmuxSessionName) return emptyResult(payload.worktreePath, tabs, activeId)

    await this.deps.toolSessions.killTmuxSession(pane.tmuxSessionName).catch(() => {})

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, (previous) => ({
        ...previous,
        isRunning: false,
        detached: false,
        tmuxSessionName: undefined,
      })),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  async reattachTmuxPane(
    sender: WebContents,
    payload: ReattachTmuxPanePayload,
  ): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane?.tmuxSessionName) return emptyResult(payload.worktreePath, tabs, activeId)

    const exists = await this.deps.toolSessions
      .hasTmuxSession(pane.tmuxSessionName)
      .catch(() => false)
    const updatedPane = exists
      ? {
          ...pane,
          ...(await this.deps.toolSessions.attachTmux(sender, {
            tmuxSessionName: pane.tmuxSessionName,
          })),
          isRunning: true,
          exitCode: null,
          detached: false,
        }
      : await this.restartPaneSnapshot(sender, payload.worktreePath, pane, {
          workspaceName: payload.options?.workspaceName,
          branch: payload.options?.branch,
        })

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, () => updatedPane),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
      restartedPane: updatedPane,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  toggleFocusedInspector(
    sender: WebContents,
    payload: ToggleFocusedInspectorPayload,
  ): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === tab.focusedPaneId,
    )
    if (!pane || !AI_TOOL_IDS.has(pane.toolId)) {
      return emptyResult(payload.worktreePath, tabs, activeId)
    }

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, pane.id, (previous) => ({
        ...previous,
        inspectorOpen: previous.inspectorOpen !== true,
      })),
    }
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  async restartPane(sender: WebContents, payload: RestartPanePayload): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    if (tabIndex < 0) {
      return emptyResult(payload.worktreePath, tabs, this.getCommandActiveTabId(sender.id, payload))
    }

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane) {
      return emptyResult(payload.worktreePath, tabs, this.getCommandActiveTabId(sender.id, payload))
    }

    const restartedPane = await this.restartPaneSnapshot(sender, payload.worktreePath, pane, {
      workspaceName: payload.options?.workspaceName,
      branch: payload.options?.branch,
    })

    tabs[tabIndex] = {
      ...tab,
      rootSplit: updatePaneSnapshot(tab.rootSplit, payload.paneId, () => restartedPane),
    }

    const activeId = this.getCommandActiveTabId(sender.id, payload) ?? tab.id
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
      restartedPane,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  async closeTab(sender: WebContents, payload: CloseTabPayload): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const idx = tabs.findIndex((tab) => tab.id === payload.tabId)
    if (idx < 0) {
      return emptyResult(payload.worktreePath, tabs, this.getCommandActiveTabId(sender.id, payload))
    }

    const [tab] = tabs.splice(idx, 1)
    this.pushClosedTab(sender.id, payload.worktreePath, tab)
    await this.cleanupPanes(sender, allPaneSnapshots(tab.rootSplit))

    let nextActiveId = this.getCommandActiveTabId(sender.id, payload)
    if (nextActiveId === payload.tabId) {
      nextActiveId = tabs.length > 0 ? tabs[Math.min(idx, tabs.length - 1)].id : null
    }

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: nextActiveId,
      closedTabId: payload.tabId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, nextActiveId)
    return result
  }

  async prepareCloseTab(
    sender: WebContents,
    payload: PrepareCloseTabPayload,
  ): Promise<TabClosePreflightResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tab = tabs.find((candidate) => candidate.id === payload.tabId)
    if (!tab || tab.suspended) return { ok: true }

    const dirtyFiles = this.dirtyEditorFiles(tab)
    if (dirtyFiles.length === 0) return { ok: true }

    const choice = await this.deps.confirmUnsavedChanges(
      sender,
      dirtyFiles.map((file) => file.filePath),
    )
    if (choice === 'cancel') return { ok: false, reason: 'cancelled' }
    if (choice === 'discard') return { ok: true }

    let failedCount = 0
    for (const file of dirtyFiles) {
      try {
        const result = await this.saveEditorFile(sender, {
          worktreePath: payload.worktreePath,
          paneId: file.paneId,
          filePath: file.filePath,
          options: {
            content: file.currentContent ?? '',
            fileLineEnding: file.fileLineEnding,
            expectedMtimeMs: file.fileMtimeMs,
          },
        })
        if (!result.ok) failedCount += 1
      } catch {
        failedCount += 1
      }
    }
    if (failedCount > 0) return { ok: false, reason: 'save-failed', failedCount }

    return { ok: true }
  }

  async prepareCloseAllForWorktree(
    sender: WebContents,
    payload: PrepareCloseAllForWorktreePayload,
  ): Promise<TabCloseAllPreflightResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const openTabs = tabs.filter((tab) => !tab.suspended)

    if (!payload.confirmedActiveProcesses) {
      const warnings = (
        await Promise.all(
          openTabs.map(async (tab) => {
            const description = await this.getActiveProcessDescription(
              allPaneSnapshots(tab.rootSplit),
            )
            return description ? { tabName: getTabDisplayName(tab), description } : null
          }),
        )
      ).filter((entry): entry is { tabName: string; description: string } => entry !== null)

      if (warnings.length > 0) {
        return { ok: false, reason: 'active-processes', warnings }
      }
    }

    const dirtyFiles = openTabs.flatMap((tab) => this.dirtyEditorFiles(tab))
    if (dirtyFiles.length === 0) return { ok: true }

    const choice = await this.deps.confirmUnsavedChanges(
      sender,
      dirtyFiles.map((file) => file.filePath),
    )
    if (choice === 'cancel') return { ok: false, reason: 'cancelled' }
    if (choice === 'discard') return { ok: true }

    let failedCount = 0
    for (const file of dirtyFiles) {
      try {
        const result = await this.saveEditorFile(sender, {
          worktreePath: payload.worktreePath,
          paneId: file.paneId,
          filePath: file.filePath,
          options: {
            content: file.currentContent ?? '',
            fileLineEnding: file.fileLineEnding,
            expectedMtimeMs: file.fileMtimeMs,
          },
        })
        if (!result.ok) failedCount += 1
      } catch {
        failedCount += 1
      }
    }
    if (failedCount > 0) return { ok: false, reason: 'save-failed', failedCount }

    return { ok: true }
  }

  async getCloseWarning(
    sender: WebContents,
    payload: GetCloseWarningPayload,
  ): Promise<CloseWarningResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const target = this.validateCloseWarningTarget(payload.target)
    const panes = this.closeWarningPanes(tabs, target)
    return { description: await this.getActiveProcessDescription(panes) }
  }

  async closePane(sender: WebContents, payload: ClosePanePayload): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const removeResult = removePaneSnapshot(tab.rootSplit, payload.paneId)
    if (!removeResult) return emptyResult(payload.worktreePath, tabs, activeId)

    await this.cleanupPanes(sender, [removeResult.removed])

    let nextActiveId = activeId
    let closedTabId: string | undefined
    if (!removeResult.tree) {
      this.pushClosedTab(sender.id, payload.worktreePath, tab)
      tabs.splice(tabIndex, 1)
      closedTabId = payload.tabId
      if (nextActiveId === payload.tabId) {
        nextActiveId = tabs.length > 0 ? tabs[Math.min(tabIndex, tabs.length - 1)].id : null
      }
    } else {
      const focusedPane = firstPaneSnapshot(removeResult.tree)
      tabs[tabIndex] = reconcileTabSnapshotIdentity(
        {
          ...tab,
          rootSplit: removeResult.tree,
          focusedPaneId: focusedPane.id,
        },
        tabs,
      )
      nextActiveId = nextActiveId ?? tab.id
    }

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: nextActiveId,
      closedTabId,
      closedPaneId: payload.paneId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, nextActiveId)
    return result
  }

  async closeAllForWorktree(
    sender: WebContents,
    payload: CloseAllForWorktreePayload,
  ): Promise<TabCommandResult> {
    this.trackSender(sender)
    const workspaceId = this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)

    await this.cleanupPanes(
      sender,
      tabs.flatMap((tab) => allPaneSnapshots(tab.rootSplit)),
      { treeWait: payload.forRemoval === true },
    )

    try {
      this.deps.layoutStore.delete(workspaceId, payload.worktreePath)
    } catch (error) {
      if (!this.deps.layoutStore.isClosed()) {
        console.error('Failed to delete layout:', error)
        throw error
      }
    }

    const result = {
      worktreePath: payload.worktreePath,
      tabs: [],
      activeTabId: null,
    }
    this.setTabState(sender, payload.worktreePath, result.tabs, result.activeTabId)
    return result
  }

  async reopenClosedTab(
    sender: WebContents,
    payload: ReopenClosedTabPayload,
  ): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const entry = this.popClosedTab(sender.id, payload.worktreePath)
    if (!entry) return emptyResult(payload.worktreePath, tabs, activeId)

    if (entry.toolId === 'diff') {
      return this.openDiffTab(sender, payload)
    }

    const pane = await this.createPane(sender, entry.toolId, payload.worktreePath, {
      workspaceName: payload.options?.workspaceName,
      branch: payload.options?.branch,
      profileId: entry.profileId,
    })
    const openedTab: TabSnapshot = {
      id: tabId(),
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

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: openedTab.id,
      openedTab,
    }
    this.setTabState(sender, payload.worktreePath, tabs, openedTab.id)
    return result
  }

  setActiveTab(sender: WebContents, payload: SetActiveTabPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const currentActiveId = this.getCommandActiveTabId(sender.id, payload)
    const nextActiveId = tabs.some((tab) => tab.id === payload.tabId)
      ? payload.tabId
      : currentActiveId

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: nextActiveId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, nextActiveId)
    return result
  }

  moveTab(sender: WebContents, payload: MoveTabPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    if (
      payload.fromIndex === payload.toIndex ||
      payload.fromIndex < 0 ||
      payload.fromIndex >= tabs.length ||
      payload.toIndex < 0 ||
      payload.toIndex >= tabs.length
    ) {
      return emptyResult(payload.worktreePath, tabs, activeId)
    }

    const [tab] = tabs.splice(payload.fromIndex, 1)
    tabs.splice(payload.toIndex, 0, tab)

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  moveTabToSplit(sender: WebContents, payload: MoveTabToSplitPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const sourceIndex = tabs.findIndex((tab) => tab.id === payload.sourceTabId)
    const targetIndex = tabs.findIndex((tab) => tab.id === payload.targetTabId)
    if (sourceIndex < 0 || targetIndex < 0 || payload.sourceTabId === payload.targetTabId) {
      return emptyResult(payload.worktreePath, tabs, activeId)
    }

    const sourceTab = tabs[sourceIndex]
    const targetTab = tabs[targetIndex]
    const nextRoot = graftSplitSnapshot(
      targetTab.rootSplit,
      payload.targetPaneId,
      payload.direction,
      sourceTab.rootSplit,
      payload.position,
    )
    if (!nextRoot) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[targetIndex] = {
      ...targetTab,
      rootSplit: nextRoot,
      focusedPaneId: firstPaneSnapshot(sourceTab.rootSplit).id,
    }
    tabs.splice(sourceIndex, 1)

    const nextActiveId = activeId === payload.sourceTabId ? payload.targetTabId : activeId
    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: nextActiveId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, nextActiveId)
    return result
  }

  movePaneToTarget(sender: WebContents, payload: MovePaneToTargetPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const sourceIndex = tabs.findIndex((tab) => tab.id === payload.sourceTabId)
    const targetIndex = tabs.findIndex((tab) => tab.id === payload.targetTabId)
    if (sourceIndex < 0 || targetIndex < 0) {
      return emptyResult(payload.worktreePath, tabs, activeId)
    }

    const sourceTab = tabs[sourceIndex]
    const targetTab = tabs[targetIndex]
    const removeResult = removePaneSnapshot(sourceTab.rootSplit, payload.sourcePaneId)
    if (!removeResult) return emptyResult(payload.worktreePath, tabs, activeId)

    const leaf: SplitSnapshot = { type: 'leaf', pane: removeResult.removed }

    if (payload.sourceTabId === payload.targetTabId) {
      if (!removeResult.tree) return emptyResult(payload.worktreePath, tabs, activeId)

      const nextRoot = graftSplitSnapshot(
        removeResult.tree,
        payload.targetPaneId,
        payload.direction,
        leaf,
        payload.position,
      )
      if (!nextRoot) return emptyResult(payload.worktreePath, tabs, activeId)

      tabs[sourceIndex] = reconcileTabSnapshotIdentity(
        {
          ...sourceTab,
          rootSplit: nextRoot,
          focusedPaneId: payload.sourcePaneId,
        },
        tabs,
      )

      const result = {
        worktreePath: payload.worktreePath,
        tabs,
        activeTabId: activeId ?? sourceTab.id,
      }
      this.setTabState(sender, payload.worktreePath, tabs, result.activeTabId)
      return result
    }

    const nextTargetRoot = graftSplitSnapshot(
      targetTab.rootSplit,
      payload.targetPaneId,
      payload.direction,
      leaf,
      payload.position,
    )
    if (!nextTargetRoot) return emptyResult(payload.worktreePath, tabs, activeId)

    const nextTabs = [...tabs]
    if (removeResult.tree) {
      nextTabs[sourceIndex] = reconcileTabSnapshotIdentity(
        {
          ...sourceTab,
          rootSplit: removeResult.tree,
          focusedPaneId: firstPaneSnapshot(removeResult.tree).id,
        },
        nextTabs,
      )
    } else {
      nextTabs.splice(sourceIndex, 1)
    }

    const nextTargetIndex = nextTabs.findIndex((tab) => tab.id === payload.targetTabId)
    if (nextTargetIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    nextTabs[nextTargetIndex] = reconcileTabSnapshotIdentity(
      {
        ...targetTab,
        rootSplit: nextTargetRoot,
        focusedPaneId: payload.sourcePaneId,
      },
      nextTabs,
    )

    const result = {
      worktreePath: payload.worktreePath,
      tabs: nextTabs,
      activeTabId: payload.targetTabId,
    }
    this.setTabState(sender, payload.worktreePath, nextTabs, result.activeTabId)
    return result
  }

  detachPaneToTab(sender: WebContents, payload: DetachPaneToTabPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const sourceIndex = tabs.findIndex((tab) => tab.id === payload.sourceTabId)
    if (sourceIndex < 0) {
      return emptyResult(payload.worktreePath, tabs, this.getCommandActiveTabId(sender.id, payload))
    }

    const sourceTab = tabs[sourceIndex]
    if (sourceTab.rootSplit.type === 'leaf') {
      return emptyResult(payload.worktreePath, tabs, this.getCommandActiveTabId(sender.id, payload))
    }

    const removeResult = removePaneSnapshot(sourceTab.rootSplit, payload.sourcePaneId)
    if (!removeResult?.tree) {
      return emptyResult(payload.worktreePath, tabs, this.getCommandActiveTabId(sender.id, payload))
    }

    tabs[sourceIndex] = reconcileTabSnapshotIdentity(
      {
        ...sourceTab,
        rootSplit: removeResult.tree,
        focusedPaneId: firstPaneSnapshot(removeResult.tree).id,
      },
      tabs,
    )

    const removed = removeResult.removed
    const openedTab: TabSnapshot = {
      id: tabId(),
      toolId: removed.toolId,
      toolName: removed.toolName,
      name: computeDisplayName(
        removed.toolName,
        payload.worktreePath,
        removed.toolId,
        tabs,
        removed.profileName,
      ),
      worktreePath: payload.worktreePath,
      rootSplit: { type: 'leaf', pane: removed },
      focusedPaneId: removed.id,
    }
    tabs.push(openedTab)

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: openedTab.id,
      openedTab,
    }
    this.setTabState(sender, payload.worktreePath, tabs, openedTab.id)
    return result
  }

  async spawnPane(sender: WebContents, payload: SpawnPanePayload): Promise<PaneSnapshot> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    return this.createPane(sender, payload.toolId, payload.worktreePath, payload.options)
  }

  async splitPane(sender: WebContents, payload: SplitPanePayload): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    if (NO_SPLIT_TOOLS.has(tab.toolId)) {
      return emptyResult(payload.worktreePath, tabs, activeId)
    }
    const target = allPaneSnapshots(tab.rootSplit).find((pane) => pane.id === payload.paneId)
    if (!target || splitDepth(tab.rootSplit) >= MAX_SPLIT_DEPTH) {
      return emptyResult(payload.worktreePath, tabs, activeId)
    }

    const pane = await this.createPane(sender, 'shell', payload.worktreePath)
    const nextRoot = splitPaneSnapshot(tab.rootSplit, payload.paneId, payload.direction, pane)
    if (!nextRoot) {
      await this.deps.toolSessions.killPty(pane.sessionId)
      return emptyResult(payload.worktreePath, tabs, activeId)
    }

    tabs[tabIndex] = {
      ...tab,
      rootSplit: nextRoot,
      focusedPaneId: pane.id,
    }

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId ?? tab.id,
    }
    this.setTabState(sender, payload.worktreePath, tabs, result.activeTabId)
    return result
  }

  focusPane(sender: WebContents, payload: FocusPanePayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const pane = allPaneSnapshots(tab.rootSplit).find(
      (candidate) => candidate.id === payload.paneId,
    )
    if (!pane) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = { ...tab, focusedPaneId: payload.paneId }

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId ?? tab.id,
    }
    this.setTabState(sender, payload.worktreePath, tabs, result.activeTabId)
    return result
  }

  navigatePaneFocus(sender: WebContents, payload: NavigatePaneFocusPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    const targetPaneId = navigatePaneSnapshot(tab.rootSplit, tab.focusedPaneId, payload.direction)
    if (!targetPaneId) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = { ...tab, focusedPaneId: targetPaneId }

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId ?? tab.id,
    }
    this.setTabState(sender, payload.worktreePath, tabs, result.activeTabId)
    return result
  }

  updateSplitRatio(sender: WebContents, payload: UpdateSplitRatioPayload): TabCommandResult {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    if (tabIndex < 0 || !Number.isFinite(payload.ratio)) {
      return emptyResult(payload.worktreePath, tabs, activeId)
    }

    const tab = tabs[tabIndex]
    const updated = updateSplitRatioSnapshot(tab.rootSplit, payload.splitId, payload.ratio)
    if (!updated.changed) return emptyResult(payload.worktreePath, tabs, activeId)

    tabs[tabIndex] = { ...tab, rootSplit: updated.split }

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId ?? tab.id,
    }
    this.setTabState(sender, payload.worktreePath, tabs, result.activeTabId)
    return result
  }

  async restoreLayout(
    sender: WebContents,
    payload: RestoreLayoutPayload,
  ): Promise<TabCommandResult & { restored: boolean }> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)

    let layout: SerializedLayout
    try {
      const parsed: unknown = JSON.parse(payload.layoutJson)
      validateSerializedLayout(parsed)
      layout = parsed
    } catch {
      return {
        ...emptyResult(
          payload.worktreePath,
          this.getCommandTabs(sender.id, payload),
          this.getCommandActiveTabId(sender.id, payload),
        ),
        restored: false,
      }
    }

    if (layout.tabs.length === 0) {
      return {
        ...emptyResult(
          payload.worktreePath,
          this.getCommandTabs(sender.id, payload),
          this.getCommandActiveTabId(sender.id, payload),
        ),
        restored: false,
      }
    }

    const activeIndex = Math.min(Math.max(0, layout.activeTabIndex), layout.tabs.length - 1)
    const restoredTabs: TabSnapshot[] = []

    for (const [index, serializedTab] of layout.tabs.entries()) {
      if (index !== activeIndex) {
        restoredTabs.push(
          this.createSuspendedTabSnapshot(payload.worktreePath, serializedTab, restoredTabs),
        )
        continue
      }

      try {
        const rootSplit = await this.restoreSerializedSplitSnapshot(
          sender,
          payload.worktreePath,
          serializedTab.rootSplit,
          {
            workspaceName: payload.options?.workspaceName,
            branch: payload.options?.branch,
          },
        )
        const focusedPane = firstPaneSnapshot(rootSplit)
        restoredTabs.push({
          id: tabId(),
          toolId: serializedTab.toolId,
          toolName: serializedTab.toolName,
          name: computeDisplayName(
            serializedTab.toolName,
            payload.worktreePath,
            serializedTab.toolId,
            restoredTabs,
          ),
          worktreePath: payload.worktreePath,
          rootSplit,
          focusedPaneId: focusedPane.id,
        })
      } catch {
        restoredTabs.push(
          this.createSuspendedTabSnapshot(payload.worktreePath, serializedTab, restoredTabs),
        )
      }
    }

    const activeTab = restoredTabs.find((tab) => !tab.suspended) ?? restoredTabs[0]
    const result = {
      worktreePath: payload.worktreePath,
      tabs: restoredTabs,
      activeTabId: activeTab.id,
      restored: true,
    }
    this.setTabState(sender, payload.worktreePath, restoredTabs, activeTab.id)
    return result
  }

  async resumeSuspendedTab(
    sender: WebContents,
    payload: ResumeSuspendedTabPayload,
  ): Promise<TabCommandResult> {
    this.trackSender(sender)
    this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.getCommandTabs(sender.id, payload)
    const activeId = this.getCommandActiveTabId(sender.id, payload)
    const tabIndex = tabs.findIndex((tab) => tab.id === payload.tabId)
    if (tabIndex < 0) return emptyResult(payload.worktreePath, tabs, activeId)

    const tab = tabs[tabIndex]
    if (!tab.suspended) return emptyResult(payload.worktreePath, tabs, activeId)

    const rootSplit = await this.restoreSerializedSplitSnapshot(
      sender,
      payload.worktreePath,
      tab.suspended,
      {
        workspaceName: payload.options?.workspaceName,
        branch: payload.options?.branch,
      },
    )
    const focusedPane = firstPaneSnapshot(rootSplit)
    const resumedTab = { ...tab }
    delete resumedTab.suspended
    tabs[tabIndex] = {
      ...resumedTab,
      rootSplit,
      focusedPaneId: focusedPane.id,
    }

    const result = {
      worktreePath: payload.worktreePath,
      tabs,
      activeTabId: activeId,
    }
    this.setTabState(sender, payload.worktreePath, tabs, activeId)
    return result
  }

  async killAll(sender: WebContents): Promise<TabStateSnapshot> {
    this.trackSender(sender)
    const tabsByWorktree = this.getAllCommandTabsByWorktree(sender.id)

    await this.cleanupPanes(
      sender,
      [...tabsByWorktree.values()].flatMap((tabs) =>
        tabs.flatMap((tab) => allPaneSnapshots(tab.rootSplit)),
      ),
    )

    this.tabsByWorktreeByWindow.delete(sender.id)
    this.activeTabIdByWorktreeByWindow.delete(sender.id)
    this.emitAppStateChanged(sender)
    return this.getSnapshot(sender.id)
  }

  focusSession(sender: WebContents, payload: FocusSessionPayload): TabCommandResult | null {
    this.trackSender(sender)
    const tabsByWorktree = this.getAllCommandTabsByWorktree(sender.id)

    for (const [worktreePath, tabs] of tabsByWorktree) {
      const tabIndex = tabs.findIndex((tab) =>
        allPaneSnapshots(tab.rootSplit).some((pane) => pane.sessionId === payload.sessionId),
      )
      if (tabIndex < 0) continue

      const tab = tabs[tabIndex]
      const pane = allPaneSnapshots(tab.rootSplit).find(
        (candidate) => candidate.sessionId === payload.sessionId,
      )
      if (!pane) continue

      tabs[tabIndex] = {
        ...tab,
        focusedPaneId: pane.id,
      }
      const result = {
        worktreePath,
        tabs,
        activeTabId: tab.id,
      }
      this.setTabState(sender, worktreePath, tabs, tab.id)
      return result
    }

    return null
  }

  getSnapshot(webContentsId: number): TabStateSnapshot {
    const tabsByWorktree: Record<string, TabSnapshot[]> = {}
    const activeTabIdByWorktree: Record<string, string | null> = {}

    const tabs = this.tabsByWorktreeByWindow.get(webContentsId)
    for (const [worktreePath, snapshots] of tabs?.entries() ?? []) {
      tabsByWorktree[worktreePath] = snapshots
    }

    const activeTabs = this.activeTabIdByWorktreeByWindow.get(webContentsId)
    for (const [worktreePath, tabId] of activeTabs?.entries() ?? []) {
      activeTabIdByWorktree[worktreePath] = tabId
    }

    return { tabsByWorktree, activeTabIdByWorktree }
  }

  saveCurrentLayout(sender: WebContents, payload: SaveCurrentLayoutPayload): void {
    const workspaceId = this.assertSenderOwnsWorktree(sender, payload.worktreePath)
    const tabs = this.tabsByWorktreeByWindow.get(sender.id)?.get(payload.worktreePath) ?? []
    const activeId =
      this.activeTabIdByWorktreeByWindow.get(sender.id)?.get(payload.worktreePath) ?? null
    const layout = this.createSerializedLayout(tabs, activeId)

    try {
      if (!layout) {
        this.deps.layoutStore.delete(workspaceId, payload.worktreePath)
        return
      }
      this.deps.layoutStore.save(workspaceId, payload.worktreePath, JSON.stringify(layout))
    } catch (error) {
      if (this.deps.layoutStore.isClosed()) return
      console.error('Failed to save layout:', error)
    }
  }

  private getCommandTabs(webContentsId: number, payload: TabCommandPayloadBase): TabSnapshot[] {
    return [...(this.tabsByWorktreeByWindow.get(webContentsId)?.get(payload.worktreePath) ?? [])]
  }

  private getCommandActiveTabId(
    webContentsId: number,
    payload: TabCommandPayloadBase,
  ): string | null {
    const stored = this.activeTabIdByWorktreeByWindow.get(webContentsId)?.get(payload.worktreePath)
    return stored ?? null
  }

  private getAllCommandTabsByWorktree(webContentsId: number): Map<string, TabSnapshot[]> {
    const tabsByWorktree = new Map<string, TabSnapshot[]>()

    for (const [worktreePath, tabs] of this.tabsByWorktreeByWindow.get(webContentsId)?.entries() ??
      []) {
      tabsByWorktree.set(worktreePath, tabs)
    }

    return tabsByWorktree
  }

  private pushClosedTab(webContentsId: number, worktreePath: string, tab: TabSnapshot): void {
    let closedTabsByWorktree = this.closedTabsByWorktreeByWindow.get(webContentsId)
    if (!closedTabsByWorktree) {
      closedTabsByWorktree = new Map()
      this.closedTabsByWorktreeByWindow.set(webContentsId, closedTabsByWorktree)
    }

    let stack = closedTabsByWorktree.get(worktreePath)
    if (!stack) {
      stack = []
      closedTabsByWorktree.set(worktreePath, stack)
    }

    stack.push({
      toolId: tab.toolId,
      toolName: tab.toolName,
      profileId: this.tabProfileId(tab),
    })
    if (stack.length > MAX_CLOSED_TABS) stack.shift()
  }

  private popClosedTab(webContentsId: number, worktreePath: string): ClosedTabEntry | undefined {
    return this.closedTabsByWorktreeByWindow.get(webContentsId)?.get(worktreePath)?.pop()
  }

  private tabProfileId(tab: TabSnapshot): string | undefined {
    if (tab.rootSplit.type === 'leaf') return tab.rootSplit.pane.profileId
    const focusedPane = allPaneSnapshots(tab.rootSplit).find(
      (pane) => pane.id === tab.focusedPaneId,
    )
    return focusedPane?.profileId
  }

  private dirtyEditorFiles(tab: TabSnapshot): DirtyEditorFile[] {
    const dirtyFiles: DirtyEditorFile[] = []
    for (const pane of allPaneSnapshots(tab.rootSplit)) {
      if (pane.paneType !== 'editor') continue
      for (const file of pane.editorFiles ?? []) {
        if (file.dirty === true) {
          dirtyFiles.push({
            paneId: pane.id,
            filePath: file.filePath,
            currentContent: file.currentContent,
            fileLineEnding: file.fileLineEnding,
            fileMtimeMs: file.fileMtimeMs,
          })
        }
      }
    }
    return dirtyFiles
  }

  private editorFileSnapshot(
    tabs: TabSnapshot[],
    paneId: string,
    filePath: string,
  ): EditorFileSnapshot | null {
    for (const tab of tabs) {
      const pane = allPaneSnapshots(tab.rootSplit).find((candidate) => candidate.id === paneId)
      const file = pane?.editorFiles?.find((candidate) => candidate.filePath === filePath)
      if (file) return file
    }
    return null
  }

  private validateCloseWarningTarget(target: unknown): CloseWarningTarget {
    if (!isRecord(target) || typeof target.tabId !== 'string') {
      throw new Error('Invalid close warning target')
    }
    if (target.kind === 'tab') {
      return { kind: 'tab', tabId: target.tabId }
    }
    if (target.kind === 'pane' && typeof target.paneId === 'string') {
      return { kind: 'pane', tabId: target.tabId, paneId: target.paneId }
    }
    throw new Error('Invalid close warning target')
  }

  private closeWarningPanes(tabs: TabSnapshot[], target: CloseWarningTarget): PaneSnapshot[] {
    const tab = tabs.find((candidate) => candidate.id === target.tabId)
    if (!tab || tab.suspended) return []
    if (target.kind === 'tab') return allPaneSnapshots(tab.rootSplit)

    const pane = allPaneSnapshots(tab.rootSplit).find((candidate) => candidate.id === target.paneId)
    return pane ? [pane] : []
  }

  private async getActiveProcessDescription(panes: PaneSnapshot[]): Promise<string | null> {
    let busyAgentSessions = 0
    let activeShell = 0

    await Promise.all(
      panes.map(async (pane) => {
        if (!pane.isRunning) return
        if (this.deps.toolSessions.isAgentTool(pane.toolId)) {
          if (this.deps.toolSessions.isAgentBusy(pane.sessionId)) busyAgentSessions++
          return
        }
        try {
          if (await this.deps.toolSessions.hasChildProcess(pane.sessionId)) activeShell++
        } catch {
          // PTY may already be gone.
        }
      }),
    )

    if (busyAgentSessions === 0 && activeShell === 0) return null

    const parts: string[] = []
    if (busyAgentSessions > 0) {
      parts.push(`${busyAgentSessions} active agent session${busyAgentSessions > 1 ? 's' : ''}`)
    }
    if (activeShell > 0) {
      parts.push(`${activeShell} running process${activeShell > 1 ? 'es' : ''}`)
    }
    return parts.join(' and ')
  }

  private setTabState(
    sender: WebContents,
    worktreePath: string,
    tabs: TabSnapshot[],
    activeTabId: string | null,
  ): void {
    let tabsByWorktree = this.tabsByWorktreeByWindow.get(sender.id)
    if (!tabsByWorktree) {
      tabsByWorktree = new Map()
      this.tabsByWorktreeByWindow.set(sender.id, tabsByWorktree)
    }
    tabsByWorktree.set(worktreePath, tabs)

    let activeTabs = this.activeTabIdByWorktreeByWindow.get(sender.id)
    if (!activeTabs) {
      activeTabs = new Map()
      this.activeTabIdByWorktreeByWindow.set(sender.id, activeTabs)
    }
    activeTabs.set(worktreePath, activeTabId)

    this.emitAppStateChanged(sender)
  }

  private trackSender(sender: WebContents): void {
    if (this.trackedWebContents.has(sender.id)) return
    this.trackedWebContents.add(sender.id)
    sender.once('destroyed', () => {
      this.tabsByWorktreeByWindow.delete(sender.id)
      this.activeTabIdByWorktreeByWindow.delete(sender.id)
      this.closedTabsByWorktreeByWindow.delete(sender.id)
      this.trackedWebContents.delete(sender.id)
    })
  }

  private emitAppStateChanged(sender: WebContents): void {
    if (sender.isDestroyed()) return
    this.deps.emitAppStateChanged(sender)
  }

  private assertSenderOwnsWorktree(sender: WebContents, worktreePath: string): string {
    const workspaceId = this.deps.resolveWorkspaceIdForWorktree(sender.id, worktreePath)
    if (!workspaceId) {
      throw new Error(`Worktree is not attached to this window: ${worktreePath}`)
    }
    return workspaceId
  }

  private createSerializedLayout(
    tabs: TabSnapshot[],
    activeTabId: string | null,
  ): SerializedLayout | null {
    const serializedTabs: SerializedLayout['tabs'] = []
    const activeIndex = tabs.findIndex((tab) => tab.id === activeTabId)
    let adjustedActiveIndex = 0

    for (const [index, tab] of tabs.entries()) {
      const rootSplit = tab.suspended
        ? cloneSerializedSplitNode(tab.suspended)
        : this.serializeSplitForLayout(tab.rootSplit)
      if (!rootSplit) continue
      if (index === activeIndex) adjustedActiveIndex = serializedTabs.length
      serializedTabs.push({ toolId: tab.toolId, toolName: tab.toolName, rootSplit })
    }

    if (serializedTabs.length === 0) return null
    return {
      tabs: serializedTabs,
      activeTabIndex: adjustedActiveIndex,
    }
  }

  private serializeSplitForLayout(node: SplitSnapshot): SerializedSplitNode | null {
    if (node.type === 'leaf') return this.serializePaneForLayout(node.pane)

    const first = this.serializeSplitForLayout(node.first)
    const second = this.serializeSplitForLayout(node.second)
    if (!first && !second) return null
    if (!first) return second
    if (!second) return first
    return {
      type: node.direction === 'horizontal' ? 'hsplit' : 'vsplit',
      first,
      second,
      ratio: node.ratio,
    }
  }

  private serializePaneForLayout(pane: PaneSnapshot): SerializedSplitNode | null {
    if (pane.paneType === 'notes' || pane.paneType === 'drawing') return null
    if (
      pane.paneType !== 'editor' &&
      pane.paneType !== 'browser' &&
      !pane.isRunning &&
      !pane.tmuxSessionName
    ) {
      return null
    }

    const leaf: SerializedSplitNode = {
      type: 'leaf',
      toolId: pane.toolId,
      toolName: pane.toolName,
    }
    const agentSessionId = this.deps.toolSessions.getAgentSessionId(pane.sessionId)
    if (agentSessionId) {
      leaf.agentSessionId = agentSessionId
      if (pane.toolId === 'claude') leaf.claudeSessionId = agentSessionId
    }
    if (pane.paneType === 'browser') {
      leaf.browserUrl = pane.url ?? ''
    }
    if (pane.paneType === 'editor') {
      leaf.filePath = pane.filePath
      const files = pane.editorFiles ?? []
      if (files.length > 0) {
        leaf.editorFiles = files.map((file) => file.filePath)
        leaf.editorActiveFile = pane.editorActiveFile ?? files[0].filePath
      } else if (pane.filePath) {
        leaf.editorFiles = [pane.filePath]
        leaf.editorActiveFile = pane.filePath
      }
    }
    if (pane.tmuxSessionName) leaf.tmuxSessionName = pane.tmuxSessionName
    if (pane.profileId) leaf.profileId = pane.profileId
    return leaf
  }

  private createSuspendedTabSnapshot(
    worktreePath: string,
    serializedTab: SerializedLayout['tabs'][number],
    existingTabs: TabSnapshot[],
  ): TabSnapshot {
    const placeholderPaneId = paneId()
    return {
      id: tabId(),
      toolId: serializedTab.toolId,
      toolName: serializedTab.toolName,
      name: computeDisplayName(
        serializedTab.toolName,
        worktreePath,
        serializedTab.toolId,
        existingTabs,
      ),
      worktreePath,
      rootSplit: {
        type: 'leaf',
        pane: {
          id: placeholderPaneId,
          sessionId: '',
          wsUrl: '',
          toolId: serializedTab.toolId,
          toolName: serializedTab.toolName,
          isRunning: false,
          exitCode: null,
          title: null,
        },
      },
      focusedPaneId: placeholderPaneId,
      suspended: serializedTab.rootSplit,
    }
  }

  private async restoreSerializedSplitSnapshot(
    sender: WebContents,
    worktreePath: string,
    node: SerializedSplitNode,
    options: { workspaceName?: string; branch?: string },
  ): Promise<SplitSnapshot> {
    if (node.type === 'leaf') {
      return {
        type: 'leaf',
        pane: await this.restoreSerializedPaneSnapshot(sender, worktreePath, node, options),
      }
    }

    const [first, second] = await Promise.all([
      this.restoreSerializedSplitSnapshot(sender, worktreePath, node.first, options),
      this.restoreSerializedSplitSnapshot(sender, worktreePath, node.second, options),
    ])
    return {
      type: 'split',
      id: splitId(),
      direction: node.type === 'hsplit' ? 'horizontal' : 'vertical',
      ratio: node.ratio,
      first,
      second,
    }
  }

  private async restoreSerializedPaneSnapshot(
    sender: WebContents,
    worktreePath: string,
    node: Extract<SerializedSplitNode, { type: 'leaf' }>,
    options: { workspaceName?: string; branch?: string },
  ): Promise<PaneSnapshot> {
    if (node.toolId === 'editor' && (node.filePath || (node.editorFiles?.length ?? 0) > 0)) {
      const files = node.editorFiles ?? (node.filePath ? [node.filePath] : [])
      const activeFile = node.editorActiveFile ?? files[0] ?? node.filePath ?? ''
      return {
        id: paneId(),
        sessionId: '',
        wsUrl: '',
        toolId: 'editor',
        toolName: node.toolName,
        isRunning: true,
        exitCode: null,
        title: null,
        paneType: 'editor',
        filePath: activeFile,
        editorFiles: files.map((filePath) => ({ filePath })),
        editorActiveFile: activeFile,
      }
    }

    if (node.toolId === 'browser') {
      return {
        id: paneId(),
        sessionId: randomUUID(),
        wsUrl: '',
        toolId: 'browser',
        toolName: node.toolName,
        isRunning: true,
        exitCode: null,
        title: null,
        paneType: 'browser',
        url: node.browserUrl,
      }
    }

    if (node.tmuxSessionName) {
      const exists = await this.deps.toolSessions
        .hasTmuxSession(node.tmuxSessionName)
        .catch(() => false)
      if (exists) {
        const result = await this.deps.toolSessions.attachTmux(sender, {
          tmuxSessionName: node.tmuxSessionName,
        })
        return {
          id: paneId(),
          sessionId: result.sessionId,
          wsUrl: result.wsUrl,
          toolId: node.toolId,
          toolName: node.toolName,
          isRunning: true,
          exitCode: null,
          title: null,
          tmuxSessionName: node.tmuxSessionName,
          profileId: node.profileId,
        }
      }
    }

    return this.createPane(sender, node.toolId, worktreePath, {
      workspaceName: options.workspaceName,
      branch: options.branch,
      resumeSessionId: node.agentSessionId ?? node.claudeSessionId,
      profileId: node.profileId,
    })
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

    if (toolId === 'diff') {
      return {
        id: paneId(),
        sessionId: '',
        wsUrl: '',
        toolId,
        toolName: 'Diff',
        isRunning: false,
        exitCode: null,
        title: null,
        paneType: 'diff',
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

  private async cleanupPanes(
    sender: WebContents,
    panes: PaneSnapshot[],
    options?: { treeWait?: boolean },
  ): Promise<void> {
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
          await this.deps.toolSessions.killPty(pane.sessionId, !!pane.tmuxSessionName, options)
        }
      }),
    )
  }
}
