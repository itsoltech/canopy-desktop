import { match } from 'ts-pattern'
import {
  type PaneSession,
  type SplitNode,
  type EditorFileState,
  createLeaf,
  nextPaneId,
  allPanes,
  findLeaf,
  splitPane as treeSplitPane,
  removePane as treeRemovePane,
  updatePane as treeUpdatePane,
  updateRatio as treeUpdateRatio,
  firstLeaf,
  navigateFrom,
  graftSubtree,
} from './splitTree'
import type { DropZone } from './dragState.svelte'
import { recordFileOpen } from './quickOpenMru.svelte'
import { workspaceState, getProjectForWorktree, selectWorktree } from './workspace.svelte'
import {
  initAgentSession,
  removeAgentSession,
  agentSessions,
  type AgentType,
} from '../agents/agentState.svelte'
import { confirm } from './dialogs.svelte'
import { getPref } from './preferences.svelte'
import { browserSessions } from '../browser/browserState.svelte'
import { sdkSessions, destroySession as destroySdkSession } from './sdkAgentSessions.svelte'
import { notesUiScope } from './notes.svelte'
import { getProfileById, getProfilesByAgent } from './profiles.svelte'
import { drawingsState } from './drawings.svelte'

function hasRemainingDrawingPanes(excludeId: string): boolean {
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      if (tab.suspended) continue
      for (const p of allPanes(tab.rootSplit)) {
        if (p.paneType === 'drawing' && p.id !== excludeId) return true
      }
    }
  }
  return false
}

function disposeEphemeralPaneState(pane: PaneSession): void {
  if (pane.paneType === 'notes') {
    delete notesUiScope[pane.sessionId]
  }
  if (pane.paneType === 'drawing' && !hasRemainingDrawingPanes(pane.id)) {
    for (const key of Object.keys(drawingsState)) delete drawingsState[key]
  }
}

// --- Active process detection ---

const AI_TOOL_IDS = new Set(['claude', 'codex', 'opencode', 'gemini'])
const SDK_TOOL_IDS = new Set(['claude-sdk', 'codex-sdk'])
export const isAiToolId = (id: string): boolean => AI_TOOL_IDS.has(id)

const ACTIVE_CLAUDE_STATUSES = new Set([
  'thinking',
  'toolCalling',
  'compacting',
  'waitingPermission',
])

async function getActiveProcessDescription(panes: PaneSession[]): Promise<string | null> {
  let busyClaude = 0
  let activeShell = 0
  let pendingAttentionCount = 0
  let streamingSdkCount = 0

  await Promise.all(
    panes.map(async (p) => {
      if (p.paneType === 'sdkChat' && p.conversationId) {
        const session = sdkSessions[p.conversationId]
        if (!session) return
        if (session.status === 'streaming') streamingSdkCount++
        const waiting = session.pendingAttention.filter((a) => a.status === 'waiting').length
        if (waiting > 0) pendingAttentionCount += waiting
        return
      }
      if (!p.isRunning) return
      if (AI_TOOL_IDS.has(p.toolId)) {
        const s = agentSessions[p.sessionId]
        if (s && ACTIVE_CLAUDE_STATUSES.has(s.status.type)) busyClaude++
      } else {
        try {
          if (await window.api.hasChildProcess(p.sessionId)) activeShell++
        } catch {
          // Ignore — PTY may already be gone
        }
      }
    }),
  )

  if (
    busyClaude === 0 &&
    activeShell === 0 &&
    pendingAttentionCount === 0 &&
    streamingSdkCount === 0
  )
    return null

  const parts: string[] = []
  if (busyClaude > 0) {
    parts.push(`${busyClaude} active Claude session${busyClaude > 1 ? 's' : ''}`)
  }
  if (activeShell > 0) {
    parts.push(`${activeShell} running process${activeShell > 1 ? 'es' : ''}`)
  }
  if (pendingAttentionCount > 0) {
    parts.push(
      `${pendingAttentionCount} pending agent prompt${pendingAttentionCount > 1 ? 's' : ''}`,
    )
  } else if (streamingSdkCount > 0) {
    parts.push(`${streamingSdkCount} streaming agent reply`)
  }
  return parts.join(' and ')
}

// --- Layout serialization types ---

interface SerializedLayout {
  tabs: SerializedTab[]
  activeTabIndex: number
}

interface SerializedTab {
  toolId: string
  toolName: string
  rootSplit: SerializedSplitNode
}

type SerializedSplitNode =
  | {
      type: 'leaf'
      toolId: string
      toolName: string
      agentSessionId?: string
      claudeSessionId?: string
      browserUrl?: string
      browserDevToolsMode?: 'bottom' | 'right'
      filePath?: string
      editorFiles?: string[]
      editorActiveFile?: string
      tmuxSessionName?: string
      profileId?: string
      paneType?: PaneSession['paneType']
      conversationId?: string
      workspaceId?: string
    }
  | { type: 'hsplit'; first: SerializedSplitNode; second: SerializedSplitNode; ratio: number }
  | { type: 'vsplit'; first: SerializedSplitNode; second: SerializedSplitNode; ratio: number }

export interface TabInfo {
  id: string
  toolId: string
  toolName: string
  name: string
  worktreePath: string
  rootSplit: SplitNode
  focusedPaneId: string
  suspended?: SerializedSplitNode
}

interface ClosedTab {
  toolId: string
  toolName: string
  worktreePath: string
  closedAt: number
}

const MAX_CLOSED_TABS = 20

export const tabsByWorktree: Record<string, TabInfo[]> = $state({})
export const activeTabId: Record<string, string> = $state({})
const closedTabs: Record<string, ClosedTab[]> = $state({})

let tabCounter = 0
function nextTabId(): string {
  return `tab-${++tabCounter}`
}

function computeDisplayName(
  toolName: string,
  worktreePath: string,
  toolId: string,
  profileName?: string,
): string {
  const baseLabel =
    profileName && profileName !== 'Default' ? `${toolName} (${profileName})` : toolName
  const existing = tabsByWorktree[worktreePath] ?? []
  const sameLabelCount = existing.filter(
    (t) => t.name === baseLabel || t.name.startsWith(`${baseLabel} #`),
  ).length
  if (sameLabelCount === 0) return baseLabel
  return `${baseLabel} #${sameLabelCount + 1}`
}

function sdkToolMeta(toolId: string): { toolId: 'claude-sdk' | 'codex-sdk'; name: string } {
  return toolId === 'codex-sdk'
    ? { toolId: 'codex-sdk', name: 'Codex Agent' }
    : { toolId: 'claude-sdk', name: 'Claude Agent' }
}

export function getActiveAgentPane(): { pane: PaneSession; tabId: string } | null {
  const path = workspaceState.selectedWorktreePath
  if (!path) return null
  const tabs = tabsByWorktree[path]
  if (!tabs) return null
  const tabId = activeTabId[path]
  const activeTab = tabs.find((t) => t.id === tabId)
  if (!activeTab) return null

  // 1) Focused pane in active tab, if it's an agent.
  const focused = findLeaf(activeTab.rootSplit, activeTab.focusedPaneId)
  if (focused && isAiToolId(focused.toolId) && focused.isRunning)
    return { pane: focused, tabId: activeTab.id }

  // 2) Any running agent pane in the active tab (common: agent split next to a Notes/Drawing pane).
  const inActive = allPanes(activeTab.rootSplit).find((p) => isAiToolId(p.toolId) && p.isRunning)
  if (inActive) return { pane: inActive, tabId: activeTab.id }

  // 3) Any running agent pane in other tabs (e.g. drawing pane in its own tab).
  for (const tab of tabs) {
    if (tab.id === tabId) continue
    const found = allPanes(tab.rootSplit).find((p) => isAiToolId(p.toolId) && p.isRunning)
    if (found) return { pane: found, tabId: tab.id }
  }
  return null
}

export async function openTool(
  toolId: string,
  worktreePath: string,
  options?: { initialUrl?: string; profileId?: string },
): Promise<TabInfo> {
  // SDK-backed agents don't go through PTY spawn — route to the in-process
  // manager via openSdkChatTab. Requires a profileId and a resolved workspace.
  if (SDK_TOOL_IDS.has(toolId)) {
    const project = getProjectForWorktree(worktreePath)
    if (!project) throw new Error(`No workspace found for ${worktreePath}`)
    if (!options?.profileId) {
      throw new Error(`openTool(${toolId}) requires a profileId`)
    }
    const tab = await openSdkChatTab(worktreePath, project.workspace.id, options.profileId, toolId)
    if (!tab) throw new Error('Failed to create SDK chat session')
    return tab
  }

  let pane: PaneSession
  const paneId = nextPaneId()
  let toolName: string
  let profileName: string | undefined

  if (toolId === 'browser') {
    const browserId = crypto.randomUUID()
    toolName = 'Browser'
    pane = {
      id: paneId,
      sessionId: browserId,
      wsUrl: '',
      toolId,
      toolName,
      isRunning: true,
      exitCode: null,
      title: null,
      paneType: 'browser',
      url: options?.initialUrl,
    }
  } else if (toolId === 'notes' || toolId === 'drawing') {
    toolName = toolId === 'notes' ? 'Notes' : 'Drawing'
    pane = {
      id: paneId,
      sessionId: crypto.randomUUID(),
      wsUrl: '',
      toolId,
      toolName,
      isRunning: true,
      exitCode: null,
      title: null,
      paneType: toolId,
    }
  } else {
    const spawnOptions: {
      workspaceName?: string
      branch?: string
      profileId?: string
    } = {}
    if (AI_TOOL_IDS.has(toolId)) {
      const project = getProjectForWorktree(worktreePath)
      spawnOptions.workspaceName = project?.workspace.name ?? workspaceState.workspace?.name ?? ''
      spawnOptions.branch = workspaceState.branch ?? undefined
      if (options?.profileId) {
        spawnOptions.profileId = options.profileId
        profileName = getProfileById(options.profileId)?.name
      }
    }
    const result = await window.api.spawnTool(toolId, worktreePath, spawnOptions)
    toolName = result.toolName
    pane = {
      id: paneId,
      sessionId: result.sessionId,
      wsUrl: result.wsUrl,
      toolId,
      toolName,
      isRunning: true,
      exitCode: null,
      title: null,
      tmuxSessionName: result.tmuxSessionName,
      profileId: options?.profileId,
      profileName,
    }
    if (AI_TOOL_IDS.has(toolId)) {
      initAgentSession(result.sessionId, toolId as AgentType)
    }
  }

  const id = nextTabId()
  const name = computeDisplayName(toolName, worktreePath, toolId, profileName)

  const tab: TabInfo = {
    id,
    toolId,
    toolName,
    name,
    worktreePath,
    rootSplit: createLeaf(pane),
    focusedPaneId: paneId,
  }

  if (!tabsByWorktree[worktreePath]) {
    tabsByWorktree[worktreePath] = []
  }
  tabsByWorktree[worktreePath].push(tab)
  activeTabId[worktreePath] = id

  scheduleSave(worktreePath)
  return tab
}

export function openTmuxTab(
  tmuxSessionName: string,
  sessionId: string,
  wsUrl: string,
  worktreePath: string,
): TabInfo {
  const paneId = nextPaneId()
  const pane: PaneSession = {
    id: paneId,
    sessionId,
    wsUrl,
    toolId: 'shell',
    toolName: 'Shell',
    isRunning: true,
    exitCode: null,
    title: null,
    tmuxSessionName,
  }

  const id = nextTabId()
  const name = computeDisplayName('Shell', worktreePath, 'shell')

  const tab: TabInfo = {
    id,
    toolId: 'shell',
    toolName: 'Shell',
    name,
    worktreePath,
    rootSplit: createLeaf(pane),
    focusedPaneId: paneId,
  }

  if (!tabsByWorktree[worktreePath]) {
    tabsByWorktree[worktreePath] = []
  }
  tabsByWorktree[worktreePath].push(tab)
  activeTabId[worktreePath] = id

  scheduleSave(worktreePath)
  return tab
}

export function openRunConfigTab(
  configName: string,
  sessionId: string,
  wsUrl: string,
  worktreePath: string,
): TabInfo {
  const paneId = nextPaneId()
  const pane: PaneSession = {
    id: paneId,
    sessionId,
    wsUrl,
    toolId: 'shell',
    toolName: 'Shell',
    isRunning: true,
    exitCode: null,
    title: null,
  }

  const id = nextTabId()
  const name = computeDisplayName(configName, worktreePath, 'shell')

  const tab: TabInfo = {
    id,
    toolId: 'shell',
    toolName: configName,
    name,
    worktreePath,
    rootSplit: createLeaf(pane),
    focusedPaneId: paneId,
  }

  if (!tabsByWorktree[worktreePath]) {
    tabsByWorktree[worktreePath] = []
  }
  tabsByWorktree[worktreePath].push(tab)
  activeTabId[worktreePath] = id

  scheduleSave(worktreePath)
  return tab
}

export async function closeTab(tabId: string): Promise<void> {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    const idx = tabs.findIndex((t) => t.id === tabId)
    if (idx === -1) continue

    const tab = tabs[idx]

    if (tab.suspended) {
      // Suspended tab: no live resources to clean up
      if (!closedTabs[path]) closedTabs[path] = []
      closedTabs[path].push({
        toolId: tab.toolId,
        toolName: tab.toolName,
        worktreePath: path,
        closedAt: Date.now(),
      })
      if (closedTabs[path].length > MAX_CLOSED_TABS) closedTabs[path].shift()

      tabsByWorktree[path].splice(idx, 1)

      if (activeTabId[path] === tabId) {
        const remaining = tabsByWorktree[path]
        if (remaining.length > 0) {
          const newIdx = Math.min(idx, remaining.length - 1)
          const newActive = remaining[newIdx]
          if (newActive.suspended && !(await resumeTab(newActive))) {
            tabsByWorktree[path].splice(newIdx, 1)
            const fallback = tabsByWorktree[path]
            if (fallback.length > 0) {
              activeTabId[path] = fallback[Math.min(newIdx, fallback.length - 1)].id
            } else {
              delete activeTabId[path]
            }
            scheduleSave(path)
            return
          }
          activeTabId[path] = newActive.id
        } else {
          delete activeTabId[path]
        }
      }

      scheduleSave(path)
      return
    }

    // Check for active processes before closing
    const panes = allPanes(tab.rootSplit)

    // Check for unsaved editor changes first
    const dirtyFiles: Array<{ pane: PaneSession; file: EditorFileState }> = []
    for (const p of panes) {
      if (p.paneType !== 'editor') continue
      for (const f of p.editorFiles ?? []) {
        if (f.dirty === true) dirtyFiles.push({ pane: p, file: f })
      }
    }
    if (dirtyFiles.length > 0) {
      const choice = await window.api.confirmUnsavedChanges(dirtyFiles.map((d) => d.file.filePath))
      if (choice === 'cancel') return
      if (choice === 'save') {
        const saveResults = await Promise.all(
          dirtyFiles.map(async ({ file }) => {
            try {
              const content = file.currentContent ?? ''
              const normalized =
                file.fileLineEnding === 'CRLF' ? content.replace(/\r?\n/g, '\r\n') : content
              const result = await window.api.writeFile(file.filePath, normalized, file.fileMtimeMs)
              if (result.ok) return { ok: true as const }
              const message =
                result.tag === 'StaleWrite'
                  ? 'File changed on disk — reload before saving'
                  : result.message
              return { ok: false as const, filePath: file.filePath, message }
            } catch (e) {
              return {
                ok: false as const,
                filePath: file.filePath,
                message: e instanceof Error ? e.message : String(e),
              }
            }
          }),
        )
        const failed = saveResults.filter((r) => !r.ok)
        if (failed.length > 0) {
          await confirm({
            title: 'Save failed',
            message: `Could not save ${failed.length} file(s). Tab close cancelled.`,
            confirmLabel: 'OK',
          })
          return
        }
      }
    }

    const description = await getActiveProcessDescription(panes)
    if (description) {
      const confirmed = await confirm({
        title: 'Close tab?',
        message: `This tab has ${description} that will be terminated.`,
        confirmLabel: 'Close Tab',
        destructive: true,
      })
      if (!confirmed) return
    }

    // Push to closed tabs stack
    if (!closedTabs[path]) closedTabs[path] = []
    closedTabs[path].push({
      toolId: tab.toolId,
      toolName: tab.toolName,
      worktreePath: path,
      closedAt: Date.now(),
    })
    if (closedTabs[path].length > MAX_CLOSED_TABS) {
      closedTabs[path].shift()
    }

    // Kill all PTYs / destroy browser views and cleanup sessions
    for (const p of panes) {
      if (agentSessions[p.sessionId]) {
        removeAgentSession(p.sessionId)
      }
      if (p.paneType === 'browser') {
        delete browserSessions[p.sessionId]
      }
      disposeEphemeralPaneState(p)
    }
    await Promise.all(
      panes
        .filter(
          (p) =>
            p.paneType !== 'editor' &&
            p.paneType !== 'notes' &&
            p.paneType !== 'drawing' &&
            p.paneType !== 'sdkChat',
        )
        .map((p) => {
          if (p.paneType === 'browser') return window.api.teardownBrowserWebview(p.sessionId)
          return window.api.killPty(p.sessionId, !!p.tmuxSessionName)
        }),
    )

    // SDK chat panes: cancel any in-flight work, close the main-side session,
    // and drop the renderer subscription. The earlier confirm() already
    // captured user intent for pending attention.
    await Promise.all(
      panes
        .filter((p) => p.paneType === 'sdkChat' && p.conversationId)
        .map(async (p) => {
          const id = p.conversationId!
          const session = sdkSessions[id]
          try {
            if (
              session?.status === 'streaming' ||
              session?.pendingAttention.some((a) => a.status === 'waiting')
            ) {
              await window.api.sdkAgent.cancel(id)
            }
            await window.api.sdkAgent.close(id)
          } catch (e) {
            console.warn('[closeTab] sdk cleanup failed', e)
          } finally {
            destroySdkSession(id)
          }
        }),
    )

    // Remove tab
    tabsByWorktree[path].splice(idx, 1)

    // If this was the active tab, switch to another
    if (activeTabId[path] === tabId) {
      const remaining = tabsByWorktree[path]
      if (remaining.length > 0) {
        const newIdx = Math.min(idx, remaining.length - 1)
        const newActive = remaining[newIdx]
        if (newActive.suspended && !(await resumeTab(newActive))) {
          tabsByWorktree[path].splice(newIdx, 1)
          const fallback = tabsByWorktree[path]
          if (fallback.length > 0) {
            activeTabId[path] = fallback[Math.min(newIdx, fallback.length - 1)].id
          } else {
            delete activeTabId[path]
          }
          scheduleSave(path)
          return
        }
        activeTabId[path] = newActive.id
      } else {
        delete activeTabId[path]
      }
    }

    scheduleSave(path)
    return
  }
}

export async function switchTab(tabId: string): Promise<void> {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    const tab = tabs.find((t) => t.id === tabId)
    if (tab) {
      if (tab.suspended && !(await resumeTab(tab))) return
      activeTabId[path] = tabId
      scheduleSave(path)
      return
    }
  }
}

export function moveTab(worktreePath: string, fromIndex: number, toIndex: number): void {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || fromIndex === toIndex) return
  if (fromIndex < 0 || fromIndex >= tabs.length) return
  if (toIndex < 0 || toIndex >= tabs.length) return
  const [tab] = tabs.splice(fromIndex, 1)
  tabs.splice(toIndex, 0, tab)
  scheduleSave(worktreePath)
}

export async function switchTabByIndex(worktreePath: string, index: number): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (tabs && index >= 0 && index < tabs.length) {
    const tab = tabs[index]
    if (tab.suspended && !(await resumeTab(tab))) return
    activeTabId[worktreePath] = tab.id
  }
}

export async function nextTab(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || tabs.length <= 1) return

  const currentId = activeTabId[worktreePath]
  const idx = tabs.findIndex((t) => t.id === currentId)
  const nextIdx = (idx + 1) % tabs.length
  const tab = tabs[nextIdx]
  if (tab.suspended && !(await resumeTab(tab))) return
  activeTabId[worktreePath] = tab.id
}

export async function prevTab(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || tabs.length <= 1) return

  const currentId = activeTabId[worktreePath]
  const idx = tabs.findIndex((t) => t.id === currentId)
  const prevIdx = (idx - 1 + tabs.length) % tabs.length
  const tab = tabs[prevIdx]
  if (tab.suspended && !(await resumeTab(tab))) return
  activeTabId[worktreePath] = tab.id
}

export async function reopenClosedTab(worktreePath: string): Promise<void> {
  const stack = closedTabs[worktreePath]
  if (!stack || stack.length === 0) return

  const entry = stack.pop()!
  await openTool(entry.toolId, worktreePath)
}

export async function openSdkChatTab(
  worktreePath: string,
  workspaceId: string,
  profileId: string,
  toolId = 'claude-sdk',
  existingConversationId?: string,
): Promise<TabInfo | null> {
  const meta = sdkToolMeta(toolId)
  let conversationId = existingConversationId
  if (!conversationId) {
    const result = await window.api.sdkAgent.create({ workspaceId, worktreePath, profileId })
    if ('error' in result) {
      console.warn('[openSdkChatTab] create failed:', result.error)
      return null
    }
    conversationId = result.conversationId
  }

  const paneId = nextPaneId()
  const pane: PaneSession = {
    id: paneId,
    sessionId: conversationId,
    wsUrl: '',
    toolId: meta.toolId,
    toolName: meta.name,
    isRunning: true,
    exitCode: null,
    title: null,
    paneType: 'sdkChat',
    conversationId,
    workspaceId,
    profileId,
  }

  const id = nextTabId()
  const tab: TabInfo = {
    id,
    toolId: meta.toolId,
    toolName: meta.name,
    name: meta.name,
    worktreePath,
    rootSplit: createLeaf(pane),
    focusedPaneId: paneId,
  }

  if (!tabsByWorktree[worktreePath]) {
    tabsByWorktree[worktreePath] = []
  }
  tabsByWorktree[worktreePath].push(tab)
  activeTabId[worktreePath] = id
  scheduleSave(worktreePath)
  return tab
}

export const pendingEditorJumps = $state<Record<string, number>>({})

export function openFile(filePath: string, worktreePath: string, opts?: { line?: number }): void {
  // Record MRU entry as a relative path (matches what Quick Open shows)
  const relPath = filePath.startsWith(worktreePath + '/')
    ? filePath.slice(worktreePath.length + 1)
    : filePath
  recordFileOpen(worktreePath, relPath)
  const tabs = (tabsByWorktree[worktreePath] ?? []).filter((t) => !t.suspended)

  // 1. File already open somewhere in a live tab — focus it
  for (const tab of tabs) {
    const panes = allPanes(tab.rootSplit)
    const existing = panes.find(
      (p) =>
        p.paneType === 'editor' &&
        (p.filePath === filePath || p.editorFiles?.some((f) => f.filePath === filePath)),
    )
    if (existing) {
      activeTabId[worktreePath] = tab.id
      tab.focusedPaneId = existing.id
      tab.rootSplit = treeUpdatePane(tab.rootSplit, existing.id, (p) => ({
        ...p,
        editorActiveFile: filePath,
        editorFiles: ensureFileInList(p.editorFiles, filePath, p.filePath),
        filePath,
      }))
      if (opts?.line) pendingEditorJumps[existing.id] = opts.line
      return
    }
  }

  // 2. Active tab in this worktree has an editor pane — add as sub-tab
  const activeId = activeTabId[worktreePath]
  const activeTab = tabs.find((t) => t.id === activeId)
  if (activeTab) {
    const panes = allPanes(activeTab.rootSplit)
    const focusedEditor =
      panes.find((p) => p.id === activeTab.focusedPaneId && p.paneType === 'editor') ??
      panes.find((p) => p.paneType === 'editor')
    if (focusedEditor) {
      activeTab.rootSplit = treeUpdatePane(activeTab.rootSplit, focusedEditor.id, (p) => ({
        ...p,
        editorActiveFile: filePath,
        editorFiles: ensureFileInList(p.editorFiles, filePath, p.filePath),
        filePath,
      }))
      activeTab.focusedPaneId = focusedEditor.id
      if (opts?.line) pendingEditorJumps[focusedEditor.id] = opts.line
      scheduleSave(worktreePath)
      return
    }
  }

  // 3. Otherwise — create a new editor tab
  const paneId = nextPaneId()
  const EDITOR_LABEL = 'Editor'
  const pane: PaneSession = {
    id: paneId,
    sessionId: '',
    wsUrl: '',
    toolId: 'editor',
    toolName: EDITOR_LABEL,
    isRunning: true,
    exitCode: null,
    title: null,
    paneType: 'editor',
    filePath,
    editorFiles: [{ filePath }],
    editorActiveFile: filePath,
  }

  const id = nextTabId()
  const name = computeDisplayName(EDITOR_LABEL, worktreePath, 'editor')

  const tab: TabInfo = {
    id,
    toolId: 'editor',
    toolName: EDITOR_LABEL,
    name,
    worktreePath,
    rootSplit: createLeaf(pane),
    focusedPaneId: paneId,
  }

  if (!tabsByWorktree[worktreePath]) {
    tabsByWorktree[worktreePath] = []
  }
  tabsByWorktree[worktreePath].push(tab)
  activeTabId[worktreePath] = id
  if (opts?.line) pendingEditorJumps[paneId] = opts.line
  scheduleSave(worktreePath)
}

function ensureFileInList(
  list: EditorFileState[] | undefined,
  filePath: string,
  legacySingle: string | undefined,
): EditorFileState[] {
  const base = list ?? (legacySingle ? [{ filePath: legacySingle }] : [])
  if (base.some((f) => f.filePath === filePath)) return base
  return [...base, { filePath }]
}

export function moveEditorFileBetweenPanes(
  sourcePaneId: string,
  targetPaneId: string,
  filePath: string,
  toIndex: number,
): void {
  if (sourcePaneId === targetPaneId) {
    moveEditorFile(targetPaneId, filePath, toIndex)
    return
  }
  // Locate both panes and capture the file state from the source
  let sourceTab: TabInfo | null = null
  let sourcePane: PaneSession | null = null
  let sourceWorktree: string | null = null
  let targetTab: TabInfo | null = null
  let targetWorktree: string | null = null
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const pane = findLeaf(tab.rootSplit, sourcePaneId)
      if (pane) {
        sourceTab = tab
        sourcePane = pane
        sourceWorktree = worktreePath
      }
      if (findLeaf(tab.rootSplit, targetPaneId)) {
        targetTab = tab
        targetWorktree = worktreePath
      }
    }
  }
  if (!sourcePane || !sourceTab || !targetTab || !sourceWorktree || !targetWorktree) return
  const movingFile = (sourcePane.editorFiles ?? []).find((f) => f.filePath === filePath)
  if (!movingFile) return

  // Remove file from source pane; if empty, collapse the pane (or close the tab)
  const remaining = (sourcePane.editorFiles ?? []).filter((f) => f.filePath !== filePath)
  if (remaining.length === 0) {
    const removed = treeRemovePane(sourceTab.rootSplit, sourcePaneId)
    if (removed) {
      if (removed.tree === null) {
        void closeTab(sourceTab.id)
      } else {
        sourceTab.rootSplit = removed.tree
        if (sourceTab.focusedPaneId === sourcePaneId) {
          sourceTab.focusedPaneId = firstLeaf(removed.tree).id
        }
        scheduleSave(sourceWorktree)
      }
    }
  } else {
    const newActive =
      sourcePane.editorActiveFile === filePath
        ? remaining[Math.max(0, remaining.length - 1)].filePath
        : sourcePane.editorActiveFile
    sourceTab.rootSplit = treeUpdatePane(sourceTab.rootSplit, sourcePaneId, (p) => ({
      ...p,
      editorFiles: remaining,
      editorActiveFile: newActive,
      filePath: newActive,
    }))
    scheduleSave(sourceWorktree)
  }

  // Add file to target pane at toIndex
  const targetPane = findLeaf(targetTab.rootSplit, targetPaneId)
  if (!targetPane) return
  const targetFiles = targetPane.editorFiles ?? []
  const clamped = Math.max(0, Math.min(toIndex, targetFiles.length))
  const next = [...targetFiles]
  // Deduplicate — if file already exists in target, just focus it
  const existingIdx = next.findIndex((f) => f.filePath === filePath)
  if (existingIdx >= 0) {
    targetTab.rootSplit = treeUpdatePane(targetTab.rootSplit, targetPaneId, (p) => ({
      ...p,
      editorActiveFile: filePath,
      filePath,
    }))
  } else {
    next.splice(clamped, 0, movingFile)
    targetTab.rootSplit = treeUpdatePane(targetTab.rootSplit, targetPaneId, (p) => ({
      ...p,
      editorFiles: next,
      editorActiveFile: filePath,
      filePath,
    }))
  }
  targetTab.focusedPaneId = targetPaneId
  activeTabId[targetWorktree] = targetTab.id
  scheduleSave(targetWorktree)
}

export function mergeTabIntoEditorPane(
  sourceTabId: string,
  targetPaneId: string,
  toIndex: number,
): void {
  let sourceTab: TabInfo | null = null
  for (const tabs of Object.values(tabsByWorktree)) {
    const tab = tabs.find((t) => t.id === sourceTabId)
    if (tab) {
      sourceTab = tab
      break
    }
  }
  if (!sourceTab) return

  const editorPanes = allPanes(sourceTab.rootSplit).filter((p) => p.paneType === 'editor')
  if (editorPanes.length === 0) return

  const moves: Array<{ sourcePaneId: string; filePath: string }> = []
  for (const p of editorPanes) {
    if (p.id === targetPaneId) continue
    const files = p.editorFiles ?? (p.filePath ? [{ filePath: p.filePath }] : [])
    for (const f of files) {
      moves.push({ sourcePaneId: p.id, filePath: f.filePath })
    }
  }

  let index = toIndex
  for (const m of moves) {
    moveEditorFileBetweenPanes(m.sourcePaneId, targetPaneId, m.filePath, index)
    index++
  }
}

export function moveEditorFile(paneId: string, filePath: string, toIndex: number): void {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      const files = existing.editorFiles ?? []
      const fromIndex = files.findIndex((f) => f.filePath === filePath)
      if (fromIndex === -1) continue
      const clamped = Math.max(0, Math.min(toIndex, files.length))
      if (clamped === fromIndex || clamped === fromIndex + 1) return
      const next = [...files]
      const [moved] = next.splice(fromIndex, 1)
      const insertAt = clamped > fromIndex ? clamped - 1 : clamped
      next.splice(insertAt, 0, moved)
      tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
        ...p,
        editorFiles: next,
      }))
      scheduleSave(worktreePath)
      return
    }
  }
}

export function setActiveEditorFile(paneId: string, filePath: string): void {
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue
      tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
        ...p,
        editorActiveFile: filePath,
        filePath,
      }))
      return
    }
  }
}

export function updateEditorFileState(
  paneId: string,
  filePath: string,
  patch: Partial<EditorFileState>,
): void {
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue
      tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
        ...p,
        editorFiles: (p.editorFiles ?? []).map((f) =>
          f.filePath === filePath ? { ...f, ...patch } : f,
        ),
      }))
      return
    }
  }
}

export function closeEditorFile(paneId: string, filePath: string): void {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue
      const remaining = (existing.editorFiles ?? []).filter((f) => f.filePath !== filePath)
      if (remaining.length === 0) {
        // No files left — close the pane (or the tab if this was the only pane)
        const removed = treeRemovePane(tab.rootSplit, paneId)
        if (removed) {
          if (removed.tree === null) {
            // Tab now empty — close it
            void closeTab(tab.id)
          } else {
            tab.rootSplit = removed.tree
            if (tab.focusedPaneId === paneId) {
              tab.focusedPaneId = firstLeaf(removed.tree).id
            }
            scheduleSave(worktreePath)
          }
        }
        return
      }
      const newActive =
        existing.editorActiveFile === filePath
          ? remaining[Math.max(0, remaining.length - 1)].filePath
          : existing.editorActiveFile
      tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
        ...p,
        editorFiles: remaining,
        editorActiveFile: newActive,
        filePath: newActive,
      }))
      scheduleSave(worktreePath)
      return
    }
  }
}

export function detachEditorFile(paneId: string, filePath: string): void {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue
      // Remove from this pane first
      const remaining = (existing.editorFiles ?? []).filter((f) => f.filePath !== filePath)
      if (remaining.length === 0) {
        // Only file — no-op (already in its own pane/tab)
        return
      }
      const newActive =
        existing.editorActiveFile === filePath
          ? remaining[Math.max(0, remaining.length - 1)].filePath
          : existing.editorActiveFile
      tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
        ...p,
        editorFiles: remaining,
        editorActiveFile: newActive,
        filePath: newActive,
      }))

      // Create new editor tab for the detached file
      const newPaneId = nextPaneId()
      const EDITOR_LABEL = 'Editor'
      const newPane: PaneSession = {
        id: newPaneId,
        sessionId: '',
        wsUrl: '',
        toolId: 'editor',
        toolName: EDITOR_LABEL,
        isRunning: true,
        exitCode: null,
        title: null,
        paneType: 'editor',
        filePath,
        editorFiles: [{ filePath }],
        editorActiveFile: filePath,
      }
      const newTabId = nextTabId()
      const name = computeDisplayName(EDITOR_LABEL, worktreePath, 'editor')
      const newTab: TabInfo = {
        id: newTabId,
        toolId: 'editor',
        toolName: EDITOR_LABEL,
        name,
        worktreePath,
        rootSplit: createLeaf(newPane),
        focusedPaneId: newPaneId,
      }
      tabsByWorktree[worktreePath].push(newTab)
      activeTabId[worktreePath] = newTabId
      scheduleSave(worktreePath)
      return
    }
  }
}

export function openDiffTab(worktreePath: string, scrollToFile?: string): void {
  // Check if a diff tab already exists — focus it
  const tabs = (tabsByWorktree[worktreePath] ?? []).filter((t) => !t.suspended)
  for (const tab of tabs) {
    const panes = allPanes(tab.rootSplit)
    const existing = panes.find((p) => p.paneType === 'diff')
    if (existing) {
      activeTabId[worktreePath] = tab.id
      tab.focusedPaneId = existing.id
      if (scrollToFile) {
        workspaceState.diffScrollTarget = { path: scrollToFile, ts: Date.now() }
      }
      return
    }
  }

  // Create single diff tab for all changes
  const paneId = nextPaneId()
  const pane: PaneSession = {
    id: paneId,
    sessionId: '',
    wsUrl: '',
    toolId: 'diff',
    toolName: 'Diff',
    isRunning: false,
    exitCode: null,
    title: null,
    paneType: 'diff',
  }

  const id = nextTabId()
  const name = computeDisplayName('Diff', worktreePath, 'diff')

  const tab: TabInfo = {
    id,
    toolId: 'diff',
    toolName: 'Diff',
    name,
    worktreePath,
    rootSplit: createLeaf(pane),
    focusedPaneId: paneId,
  }

  if (!tabsByWorktree[worktreePath]) {
    tabsByWorktree[worktreePath] = []
  }
  tabsByWorktree[worktreePath].push(tab)
  activeTabId[worktreePath] = id
  scheduleSave(worktreePath)

  if (scrollToFile) {
    workspaceState.diffScrollTarget = { path: scrollToFile, ts: Date.now() }
  }
}

export function getActiveTab(worktreePath: string): TabInfo | null {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return null
  const id = activeTabId[worktreePath]
  return tabs.find((t) => t.id === id) ?? null
}

export function getTabsForWorktree(worktreePath: string): TabInfo[] {
  return tabsByWorktree[worktreePath] ?? []
}

export function getRunningCountByTool(worktreePath: string, toolId: string): number {
  const tabs = (tabsByWorktree[worktreePath] ?? []).filter((t) => !t.suspended)
  let count = 0
  for (const tab of tabs) {
    for (const pane of allPanes(tab.rootSplit)) {
      if (pane.toolId === toolId && pane.isRunning) count++
    }
  }
  return count
}

export async function handlePtyExit(
  sessionId: string,
  exitCode: number,
  tmuxSessionName?: string,
): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const pane = panes.find((p) => p.sessionId === sessionId)
      if (pane) {
        const tmuxName = tmuxSessionName || pane.tmuxSessionName
        // Check if the tmux session is actually still alive before marking detached
        let detached = false
        if (tmuxName) {
          detached = await window.api.tmuxHasSession(tmuxName).catch(() => false)
        }
        tab.rootSplit = treeUpdatePane(tab.rootSplit, pane.id, (p) => ({
          ...p,
          isRunning: false,
          exitCode,
          detached,
          tmuxSessionName: detached ? p.tmuxSessionName : undefined,
        }))
        // Persist updated state so dead tabs are excluded from saved layout
        scheduleSave(worktreePath)
        return
      }
    }
  }
}

export async function restartPane(
  worktreePath: string,
  tabId: string,
  paneId: string,
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return

  const panes = allPanes(tab.rootSplit)
  const pane = panes.find((p) => p.id === paneId)
  if (!pane) return

  await match(pane)
    .with({ paneType: 'editor' }, () => {
      // Editor panes have no session to restart. The component re-reads on mount.
    })
    .with({ paneType: 'browser' }, async (p) => {
      const oldUrl = p.url
      try {
        await window.api.teardownBrowserWebview(p.sessionId)
      } catch {
        // Already destroyed
      }
      const newBrowserId = crypto.randomUUID()
      tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (prev) => ({
        ...prev,
        sessionId: newBrowserId,
        url: oldUrl,
        isRunning: true,
        exitCode: null,
        title: null,
      }))
    })
    .when(
      (p) => SDK_TOOL_IDS.has(p.toolId),
      async (p) => {
        const meta = sdkToolMeta(p.toolId)
        const project = getProjectForWorktree(worktreePath)
        const workspaceId = p.workspaceId ?? project?.workspace.id
        const profileId = p.profileId ?? getProfilesByAgent(meta.toolId)[0]?.id
        const existing = !p.conversationId
          ? await findLatestSdkConversation(workspaceId, worktreePath, profileId)
          : null
        let conversationId = p.conversationId ?? existing?.id
        const resolvedProfileId = p.profileId ?? existing?.agentProfileId ?? profileId

        if (!conversationId && workspaceId && resolvedProfileId) {
          const result = await window.api.sdkAgent.create({
            workspaceId,
            worktreePath,
            profileId: resolvedProfileId,
          })
          if ('error' in result) return
          conversationId = result.conversationId
        }
        if (!conversationId || !workspaceId || !resolvedProfileId) return

        tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (prev) => ({
          ...prev,
          sessionId: conversationId,
          wsUrl: '',
          toolId: meta.toolId,
          toolName: meta.name,
          isRunning: true,
          exitCode: null,
          title: null,
          paneType: 'sdkChat',
          conversationId,
          workspaceId,
          profileId: resolvedProfileId,
        }))
      },
    )
    .when(
      (p) => !!(p.tmuxSessionName && p.detached),
      async (p) => {
        const exists = await window.api.tmuxHasSession(p.tmuxSessionName!)
        if (exists) {
          const result = await window.api.tmuxAttach(p.tmuxSessionName!)
          tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (prev) => ({
            ...prev,
            sessionId: result.sessionId,
            wsUrl: result.wsUrl,
            isRunning: true,
            exitCode: null,
            detached: false,
          }))
        } else {
          const result = await window.api.spawnTool(p.toolId, worktreePath)
          tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (prev) => ({
            ...prev,
            sessionId: result.sessionId,
            wsUrl: result.wsUrl,
            isRunning: true,
            exitCode: null,
            detached: false,
            tmuxSessionName: result.tmuxSessionName,
          }))
        }
      },
    )
    .otherwise(async (p) => {
      try {
        await window.api.killPty(p.sessionId)
      } catch {
        // Already exited or cleaned up
      }

      if (AI_TOOL_IDS.has(p.toolId)) {
        removeAgentSession(p.sessionId)
      }

      const options: { workspaceName?: string; branch?: string } = {}
      if (AI_TOOL_IDS.has(p.toolId)) {
        options.workspaceName = workspaceState.workspace?.name ?? ''
        options.branch = workspaceState.branch ?? undefined
      }
      const result = await window.api.spawnTool(p.toolId, worktreePath, options)

      if (AI_TOOL_IDS.has(p.toolId)) {
        initAgentSession(result.sessionId, p.toolId as AgentType)
      }

      tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (prev) => ({
        ...prev,
        sessionId: result.sessionId,
        wsUrl: result.wsUrl,
        isRunning: true,
        exitCode: null,
        title: null,
        tmuxSessionName: result.tmuxSessionName,
      }))
    })

  scheduleSave(worktreePath)
}

export async function reattachTmuxPane(
  worktreePath: string,
  tabId: string,
  paneId: string,
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return
  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return
  const panes = allPanes(tab.rootSplit)
  const pane = panes.find((p) => p.id === paneId)
  if (!pane?.tmuxSessionName) return

  const exists = await window.api.tmuxHasSession(pane.tmuxSessionName)
  if (exists) {
    const result = await window.api.tmuxAttach(pane.tmuxSessionName)
    tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
      ...p,
      sessionId: result.sessionId,
      wsUrl: result.wsUrl,
      isRunning: true,
      exitCode: null,
      detached: false,
    }))
  } else {
    // Tmux session gone, spawn fresh
    const result = await window.api.spawnTool(pane.toolId, worktreePath)
    tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
      ...p,
      sessionId: result.sessionId,
      wsUrl: result.wsUrl,
      isRunning: true,
      exitCode: null,
      detached: false,
      tmuxSessionName: result.tmuxSessionName,
    }))
  }
  scheduleSave(worktreePath)
}

export async function killTmuxPane(
  worktreePath: string,
  tabId: string,
  paneId: string,
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return
  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return
  const panes = allPanes(tab.rootSplit)
  const pane = panes.find((p) => p.id === paneId)
  if (!pane?.tmuxSessionName) return

  try {
    await window.api.tmuxKillSession(pane.tmuxSessionName)
  } catch {
    // Session may already be gone
  }
  tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
    ...p,
    isRunning: false,
    detached: false,
    tmuxSessionName: undefined,
  }))
  scheduleSave(worktreePath)
}

export async function restartTab(tabId: string): Promise<void> {
  for (const tabs of Object.values(tabsByWorktree)) {
    const tab = tabs.find((t) => t.id === tabId)
    if (!tab) continue

    if (tab.suspended) {
      await resumeTab(tab)
      return
    }

    // Restart the focused pane
    await restartPane(tab.worktreePath, tabId, tab.focusedPaneId)
    return
  }
}

export async function ensureDefaultTab(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (tabs && tabs.length > 0) return
  await openTool(getPref('newTab.toolId', 'shell'), worktreePath)
}

async function resumeTab(tab: TabInfo): Promise<boolean> {
  if (!tab.suspended) return true
  const serialized = tab.suspended
  try {
    const rootSplit = await restoreSplitNode(serialized, tab.worktreePath)
    tab.rootSplit = rootSplit
    tab.focusedPaneId = firstLeaf(rootSplit).id
    tab.suspended = undefined
    return true
  } catch (err) {
    console.error('Failed to resume suspended tab:', err)
    return false
  }
}

export async function closeAllTabsForWorktree(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || tabs.length === 0) return

  const allSessions = tabs.filter((t) => !t.suspended).flatMap((t) => allPanes(t.rootSplit))
  for (const p of allSessions) {
    if (agentSessions[p.sessionId]) removeAgentSession(p.sessionId)
    if (p.paneType === 'browser') delete browserSessions[p.sessionId]
    disposeEphemeralPaneState(p)
  }
  await Promise.allSettled(
    allSessions
      .filter(
        (p) =>
          p.paneType !== 'editor' &&
          p.paneType !== 'notes' &&
          p.paneType !== 'drawing' &&
          p.paneType !== 'sdkChat',
      )
      .map((p) => {
        if (p.paneType === 'browser') return window.api.teardownBrowserWebview(p.sessionId)
        return window.api.killPty(p.sessionId, true)
      }),
  )

  if (saveTimers[worktreePath]) {
    clearTimeout(saveTimers[worktreePath])
    delete saveTimers[worktreePath]
  }
  delete tabsByWorktree[worktreePath]
  delete activeTabId[worktreePath]

  const wsId = getProjectForWorktree(worktreePath)?.workspace.id
  if (wsId) {
    window.api.deleteLayout(wsId, worktreePath).catch(() => {})
  }
}

export async function killAllTabs(): Promise<void> {
  const allTabsList = Object.values(tabsByWorktree).flat()
  const allSessions = allTabsList.filter((t) => !t.suspended).flatMap((t) => allPanes(t.rootSplit))
  for (const p of allSessions) disposeEphemeralPaneState(p)
  await Promise.all(
    allSessions
      .filter((p) => p.paneType !== 'editor' && p.paneType !== 'notes' && p.paneType !== 'drawing')
      .map((p) => {
        if (p.paneType === 'browser') return window.api.teardownBrowserWebview(p.sessionId)
        return window.api.killPty(p.sessionId, true)
      }),
  )
  for (const path of Object.keys(tabsByWorktree)) {
    delete tabsByWorktree[path]
    delete activeTabId[path]
  }
}

export function focusSessionByPtyId(ptySessionId: string): boolean {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const pane = panes.find((p) => p.sessionId === ptySessionId)
      if (pane) {
        // Use selectWorktree to fully update project context (sidebar, git info, etc.)
        selectWorktree(path).catch((err) => {
          console.error('[tabs] selectWorktree failed after focusSession:', err)
        })
        activeTabId[path] = tab.id
        tab.focusedPaneId = pane.id
        return true
      }
    }
  }
  return false
}

export function getAllTabs(): TabInfo[] {
  return Object.values(tabsByWorktree).flat()
}

export function findWorktreeForSession(sessionId: string): string | null {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      if (allPanes(tab.rootSplit).some((p) => p.sessionId === sessionId)) return path
    }
  }
  return null
}

export interface AiSessionInfo {
  sessionId: string
  tabName: string
  toolId: string
  status: string
}

export function getAiSessions(worktreePath: string): AiSessionInfo[] {
  const tabs = tabsByWorktree[worktreePath] ?? []
  const result: AiSessionInfo[] = []
  for (const tab of tabs) {
    const panes = allPanes(tab.rootSplit)
    for (const p of panes) {
      if (AI_TOOL_IDS.has(p.toolId) && p.isRunning) {
        const cs = agentSessions[p.sessionId] ?? null
        result.push({
          sessionId: p.sessionId,
          tabName: tab.name,
          toolId: p.toolId,
          status: cs?.status?.type ?? 'running',
        })
      }
    }
  }
  return result
}

export function getTabDisplayName(tab: TabInfo): string {
  const focused = findLeaf(tab.rootSplit, tab.focusedPaneId)
  return focused?.title || tab.name
}

export function isTabDirty(tab: TabInfo): boolean {
  return allPanes(tab.rootSplit).some(
    (p) => p.paneType === 'editor' && (p.editorFiles ?? []).some((f) => f.dirty === true),
  )
}

export function getActivePtySessionId(): string | null {
  const path = workspaceState.selectedWorktreePath
  if (!path) return null
  const tabId = activeTabId[path]
  const tab = tabsByWorktree[path]?.find((t) => t.id === tabId)
  if (!tab) return null
  const focused = findLeaf(tab.rootSplit, tab.focusedPaneId)
  return focused?.sessionId ?? null
}

export function toggleFocusedInspector(): void {
  const path = workspaceState.selectedWorktreePath
  if (!path) return
  const tabId = activeTabId[path]
  const tab = tabsByWorktree[path]?.find((t) => t.id === tabId)
  if (!tab) return
  const pane = findLeaf(tab.rootSplit, tab.focusedPaneId)
  if (pane && AI_TOOL_IDS.has(pane.toolId)) {
    tab.rootSplit = treeUpdatePane(tab.rootSplit, pane.id, (p) => ({
      ...p,
      inspectorOpen: p.inspectorOpen === false,
    }))
  }
}

export function updateTmuxSessionName(oldName: string, newName: string): void {
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const pane = panes.find((p) => p.tmuxSessionName === oldName)
      if (pane) {
        tab.rootSplit = treeUpdatePane(tab.rootSplit, pane.id, (p) => ({
          ...p,
          tmuxSessionName: newName,
        }))
        scheduleSave(tab.worktreePath)
        return
      }
    }
  }
}

export function updatePaneTitle(sessionId: string, title: string): void {
  if (!title) return
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const pane = panes.find((p) => p.sessionId === sessionId)
      if (pane) {
        tab.rootSplit = treeUpdatePane(tab.rootSplit, pane.id, (p) => ({
          ...p,
          title,
        }))
        // Forward title to main process for the notch overlay
        if (agentSessions[pane.sessionId]) {
          window.api.updateAgentTitle(sessionId, title)
        }
        return
      }
    }
  }
}

export function updateBrowserPaneUrl(sessionId: string, url: string): void {
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const pane = panes.find((p) => p.sessionId === sessionId)
      if (pane) {
        tab.rootSplit = treeUpdatePane(tab.rootSplit, pane.id, (p) => ({
          ...p,
          url,
        }))
        scheduleSave(tab.worktreePath)
        return
      }
    }
  }
}

export function updateEditorPaneState(paneId: string, patch: Partial<PaneSession>): void {
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({ ...p, ...patch }))
      return
    }
  }
}

export function findEditorPane(paneId: string): PaneSession | null {
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      const pane = findLeaf(tab.rootSplit, paneId)
      if (pane) return pane
    }
  }
  return null
}

// --- Tab identity reconciliation ---

function reconcileTabIdentity(tab: TabInfo): void {
  const focused = findLeaf(tab.rootSplit, tab.focusedPaneId)
  if (!focused || tab.toolId === focused.toolId) return
  tab.toolId = focused.toolId
  tab.toolName = focused.toolName
  const tabs = tabsByWorktree[tab.worktreePath] ?? []
  const sameCount = tabs.filter((t) => t !== tab && t.toolId === focused.toolId).length
  tab.name = sameCount === 0 ? focused.toolName : `${focused.toolName} #${sameCount + 1}`
}

// --- Split pane operations ---

const NO_SPLIT_TOOLS = new Set(['claude', 'codex', 'opencode', 'gemini'])

export async function splitFocusedPane(
  worktreePath: string,
  direction: 'hsplit' | 'vsplit',
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const tabId = activeTabId[worktreePath]
  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return

  // AI tools don't support splitting
  if (NO_SPLIT_TOOLS.has(tab.toolId)) return

  // Spawn a new shell session in the same worktree
  const result = await window.api.spawnTool('shell', worktreePath)
  const paneId = nextPaneId()

  const newPane: PaneSession = {
    id: paneId,
    sessionId: result.sessionId,
    wsUrl: result.wsUrl,
    toolId: 'shell',
    toolName: result.toolName,
    isRunning: true,
    exitCode: null,
    title: null,
  }

  const newTree = treeSplitPane(tab.rootSplit, tab.focusedPaneId, direction, newPane)
  if (!newTree) {
    // Max depth reached — kill the spawned PTY
    await window.api.killPty(result.sessionId)
    return
  }

  tab.rootSplit = newTree
  tab.focusedPaneId = paneId
  scheduleSave(worktreePath)
}

export async function closePane(
  worktreePath: string,
  tabId: string,
  paneId: string,
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return

  const pane = findLeaf(tab.rootSplit, paneId)
  if (!pane) return

  // Check if this pane has active process before closing
  if (pane.isRunning) {
    const description = await getActiveProcessDescription([pane])
    if (description) {
      const confirmed = await confirm({
        title: 'Close pane?',
        message: `This pane has ${description} that will be terminated.`,
        confirmLabel: 'Close Pane',
        destructive: true,
      })
      if (!confirmed) return
    }
  }

  const result = treeRemovePane(tab.rootSplit, paneId)
  if (!result) return
  disposeEphemeralPaneState(result.removed)

  // Kill the removed pane's PTY or destroy browser view (editor/notes/drawing panes have no session)
  if (
    result.removed.paneType === 'editor' ||
    result.removed.paneType === 'notes' ||
    result.removed.paneType === 'drawing'
  ) {
    // No-op — ephemeral or filesystem-backed only
  } else if (result.removed.paneType === 'browser') {
    delete browserSessions[result.removed.sessionId]
    await window.api.teardownBrowserWebview(result.removed.sessionId)
  } else {
    await window.api.killPty(result.removed.sessionId, !!result.removed.tmuxSessionName)
  }

  if (!result.tree) {
    // Last pane — close the tab entirely
    if (!closedTabs[worktreePath]) closedTabs[worktreePath] = []
    closedTabs[worktreePath].push({
      toolId: tab.toolId,
      toolName: tab.toolName,
      worktreePath,
      closedAt: Date.now(),
    })
    if (closedTabs[worktreePath].length > MAX_CLOSED_TABS) {
      closedTabs[worktreePath].shift()
    }

    const idx = tabs.findIndex((t) => t.id === tabId)
    tabsByWorktree[worktreePath].splice(idx, 1)

    const remaining = tabsByWorktree[worktreePath]
    if (remaining.length > 0) {
      const newIdx = Math.min(idx, remaining.length - 1)
      activeTabId[worktreePath] = remaining[newIdx].id
    } else {
      delete activeTabId[worktreePath]
    }
  } else {
    tab.rootSplit = result.tree
    // Focus the first leaf in the remaining tree
    tab.focusedPaneId = firstLeaf(result.tree).id
    // Update tab identity to match focused pane (e.g., after closing Claude, only Shell remains)
    reconcileTabIdentity(tab)
  }
  scheduleSave(worktreePath)
}

export async function closeFocusedPane(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const tabId = activeTabId[worktreePath]
  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return

  await closePane(worktreePath, tab.id, tab.focusedPaneId)
}

export function navigatePaneFocus(
  worktreePath: string,
  direction: 'left' | 'right' | 'up' | 'down',
): void {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const tabId = activeTabId[worktreePath]
  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return

  const target = navigateFrom(tab.rootSplit, tab.focusedPaneId, direction)
  if (target) {
    tab.focusedPaneId = target
  }
}

export function focusPane(_worktreePath: string, tabId: string, paneId: string): void {
  for (const tabs of Object.values(tabsByWorktree)) {
    const tab = tabs.find((t) => t.id === tabId)
    if (tab) {
      tab.focusedPaneId = paneId
      return
    }
  }
}

export function updateSplitRatio(
  _worktreePath: string,
  tabId: string,
  splitId: string,
  ratio: number,
): void {
  for (const tabs of Object.values(tabsByWorktree)) {
    const tab = tabs.find((t) => t.id === tabId)
    if (tab) {
      tab.rootSplit = treeUpdateRatio(tab.rootSplit, splitId, ratio)
      scheduleSave(tab.worktreePath)
      return
    }
  }
}

// --- Move tab to split ---

function mapZone(zone: DropZone): { direction: 'hsplit' | 'vsplit'; position: 'first' | 'second' } {
  return match(zone)
    .with('left', () => ({ direction: 'vsplit' as const, position: 'first' as const }))
    .with('right', () => ({ direction: 'vsplit' as const, position: 'second' as const }))
    .with('top', () => ({ direction: 'hsplit' as const, position: 'first' as const }))
    .with('bottom', () => ({ direction: 'hsplit' as const, position: 'second' as const }))
    .exhaustive()
}

export async function moveTabToSplit(
  worktreePath: string,
  sourceTabId: string,
  targetTabId: string,
  targetPaneId: string,
  zone: DropZone,
): Promise<boolean> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return false

  const sourceTab = tabs.find((t) => t.id === sourceTabId)
  const targetTab = tabs.find((t) => t.id === targetTabId)
  if (!sourceTab || !targetTab || sourceTabId === targetTabId) return false

  if (sourceTab.suspended && !(await resumeTab(sourceTab))) return false
  if (targetTab.suspended && !(await resumeTab(targetTab))) return false

  const { direction, position } = mapZone(zone)
  const newTree = graftSubtree(
    targetTab.rootSplit,
    targetPaneId,
    direction,
    sourceTab.rootSplit,
    position,
  )
  if (!newTree) return false

  // Update target tab with grafted tree
  targetTab.rootSplit = newTree
  targetTab.focusedPaneId = firstLeaf(sourceTab.rootSplit).id

  // Remove source tab WITHOUT killing sessions — they are relocated, not closed
  const sourceIdx = tabs.findIndex((t) => t.id === sourceTabId)
  tabs.splice(sourceIdx, 1)

  // If source was active, switch to target
  if (activeTabId[worktreePath] === sourceTabId) {
    activeTabId[worktreePath] = targetTabId
  }

  scheduleSave(worktreePath)
  return true
}

// --- Move pane within or across tabs ---

export function movePaneToTarget(
  worktreePath: string,
  sourceTabId: string,
  sourcePaneId: string,
  targetTabId: string,
  targetPaneId: string,
  zone: DropZone,
): boolean {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return false

  const sourceTab = tabs.find((t) => t.id === sourceTabId)
  const targetTab = tabs.find((t) => t.id === targetTabId)
  if (!sourceTab || !targetTab) return false

  // Extract the source pane from its tree
  const removeResult = treeRemovePane(sourceTab.rootSplit, sourcePaneId)
  if (!removeResult) return false

  const leaf = createLeaf(removeResult.removed)
  const { direction, position } = mapZone(zone)

  if (sourceTabId === targetTabId) {
    // Same-tab reorder: removeResult.tree is the tree without the source pane
    if (!removeResult.tree) return false // was the only pane — should not happen
    const newTree = graftSubtree(removeResult.tree, targetPaneId, direction, leaf, position)
    if (!newTree) {
      return false
    }
    sourceTab.rootSplit = newTree
    sourceTab.focusedPaneId = sourcePaneId
    reconcileTabIdentity(sourceTab)
  } else {
    // Cross-tab move: graft into target tab
    const newTargetTree = graftSubtree(targetTab.rootSplit, targetPaneId, direction, leaf, position)
    if (!newTargetTree) {
      return false
    }

    targetTab.rootSplit = newTargetTree
    targetTab.focusedPaneId = sourcePaneId
    reconcileTabIdentity(targetTab)

    if (!removeResult.tree) {
      // Source tab had only this pane — remove the tab
      const sourceIdx = tabs.findIndex((t) => t.id === sourceTabId)
      tabs.splice(sourceIdx, 1)
      if (activeTabId[worktreePath] === sourceTabId) {
        activeTabId[worktreePath] = targetTabId
      }
    } else {
      sourceTab.rootSplit = removeResult.tree
      sourceTab.focusedPaneId = firstLeaf(removeResult.tree).id
      reconcileTabIdentity(sourceTab)
    }

    activeTabId[worktreePath] = targetTabId
  }

  scheduleSave(worktreePath)
  return true
}

export function detachPaneToTab(
  worktreePath: string,
  sourceTabId: string,
  sourcePaneId: string,
): boolean {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return false

  const sourceTab = tabs.find((t) => t.id === sourceTabId)
  if (!sourceTab) return false

  // Already a standalone tab — nothing to detach
  if (sourceTab.rootSplit.type === 'leaf') return false

  const removeResult = treeRemovePane(sourceTab.rootSplit, sourcePaneId)
  if (!removeResult || !removeResult.tree) return false

  // Update source tab
  sourceTab.rootSplit = removeResult.tree
  sourceTab.focusedPaneId = firstLeaf(removeResult.tree).id
  reconcileTabIdentity(sourceTab)

  // Create new tab for the detached pane
  const removed = removeResult.removed
  const newTab: TabInfo = {
    id: nextTabId(),
    toolId: removed.toolId,
    toolName: removed.toolName,
    name: computeDisplayName(removed.toolName, worktreePath, removed.toolId),
    worktreePath,
    rootSplit: createLeaf(removed),
    focusedPaneId: removed.id,
  }

  tabs.push(newTab)
  activeTabId[worktreePath] = newTab.id

  scheduleSave(worktreePath)
  return true
}

// --- Layout persistence ---

const saveTimers: Record<string, ReturnType<typeof setTimeout>> = {}

function scheduleSave(worktreePath: string): void {
  if (saveTimers[worktreePath]) clearTimeout(saveTimers[worktreePath])
  saveTimers[worktreePath] = setTimeout(() => {
    delete saveTimers[worktreePath]
    saveLayoutForWorktree(worktreePath)
  }, 500)
}

function serializeSplitNode(node: SplitNode): SerializedSplitNode | null {
  if (node.type === 'leaf') {
    // Notes and drawing panes are inherently ephemeral — never persist
    if (node.pane.paneType === 'notes' || node.pane.paneType === 'drawing') {
      return null
    }
    // Skip dead terminal panes — they would respawn as new sessions on restore
    if (
      node.pane.paneType !== 'editor' &&
      node.pane.paneType !== 'browser' &&
      !node.pane.isRunning &&
      !node.pane.tmuxSessionName
    ) {
      return null
    }
    const leaf: SerializedSplitNode = {
      type: 'leaf',
      toolId: node.pane.toolId,
      toolName: node.pane.toolName,
    }
    if (agentSessions[node.pane.sessionId]) {
      const csid = agentSessions[node.pane.sessionId]?.agentSessionId
      if (csid) leaf.agentSessionId = csid
      // Backward compat: also write claudeSessionId for claude agents
      if (node.pane.toolId === 'claude' && csid) leaf.claudeSessionId = csid
    }
    if (node.pane.paneType === 'browser') {
      leaf.browserUrl = node.pane.url ?? ''
      const bs = browserSessions[node.pane.sessionId]
      if (bs) leaf.browserDevToolsMode = bs.devToolsMode
    }
    if (node.pane.paneType === 'sdkChat') {
      leaf.paneType = 'sdkChat'
      leaf.conversationId = node.pane.conversationId
      leaf.workspaceId = node.pane.workspaceId
    }
    if (node.pane.paneType === 'editor') {
      leaf.filePath = node.pane.filePath
      const files = node.pane.editorFiles ?? []
      if (files.length > 0) {
        leaf.editorFiles = files.map((f) => f.filePath)
        leaf.editorActiveFile = node.pane.editorActiveFile ?? files[0].filePath
      } else if (node.pane.filePath) {
        leaf.editorFiles = [node.pane.filePath]
        leaf.editorActiveFile = node.pane.filePath
      }
    }
    if (node.pane.tmuxSessionName) {
      leaf.tmuxSessionName = node.pane.tmuxSessionName
    }
    if (node.pane.profileId) {
      leaf.profileId = node.pane.profileId
    }
    return leaf
  }
  // Collapse splits when one or both children are dead
  const first = serializeSplitNode(node.first)
  const second = serializeSplitNode(node.second)
  if (!first && !second) return null
  if (!first) return second
  if (!second) return first
  return {
    type: node.type,
    first,
    second,
    ratio: node.ratio,
  }
}

function saveLayoutForWorktree(worktreePath: string): void {
  const tabs = tabsByWorktree[worktreePath]
  const wsId = getProjectForWorktree(worktreePath)?.workspace.id
  if (!tabs || !wsId) return

  const activeId = activeTabId[worktreePath]
  const activeIndex = tabs.findIndex((t) => t.id === activeId)

  // Filter out tabs where all panes are dead (stopped processes, no tmux session)
  const serializedTabs: Array<{
    toolId: string
    toolName: string
    rootSplit: SerializedSplitNode
  }> = []
  let adjustedActiveIndex = 0
  for (let i = 0; i < tabs.length; i++) {
    const tab = tabs[i]
    const rootSplit = tab.suspended ? tab.suspended : serializeSplitNode(tab.rootSplit)
    if (!rootSplit) continue
    if (i === activeIndex) adjustedActiveIndex = serializedTabs.length
    serializedTabs.push({ toolId: tab.toolId, toolName: tab.toolName, rootSplit })
  }

  if (serializedTabs.length === 0) {
    window.api.deleteLayout(wsId, worktreePath).catch(() => {
      // Ignore delete errors silently
    })
    return
  }

  const layout: SerializedLayout = {
    tabs: serializedTabs,
    activeTabIndex: adjustedActiveIndex,
  }

  window.api.saveLayout(wsId, worktreePath, JSON.stringify(layout)).catch(() => {
    // Ignore save errors silently
  })
}

export function saveAllLayouts(): void {
  for (const path of Object.keys(saveTimers)) {
    clearTimeout(saveTimers[path])
    delete saveTimers[path]
  }
  for (const path of Object.keys(tabsByWorktree)) {
    saveLayoutForWorktree(path)
  }
}

async function findLatestSdkConversation(
  workspaceId: string | undefined,
  worktreePath: string,
  profileId?: string,
): Promise<{ id: string; agentProfileId: string } | null> {
  if (!workspaceId) return null
  try {
    const conversations = await window.api.sdkAgent.list(workspaceId)
    const match = conversations.find(
      (conversation) =>
        conversation.worktreePath === worktreePath &&
        (!profileId || conversation.agentProfileId === profileId),
    )
    return match ? { id: match.id, agentProfileId: match.agentProfileId } : null
  } catch (e) {
    console.warn('[tabs] failed to resolve SDK conversation for restore', e)
    return null
  }
}

async function restoreSplitNode(
  node: SerializedSplitNode,
  worktreePath: string,
): Promise<SplitNode> {
  if (node.type === 'leaf') {
    const paneId = nextPaneId()
    let pane: PaneSession

    if (node.toolId === 'editor' && (node.filePath || (node.editorFiles?.length ?? 0) > 0)) {
      const files = node.editorFiles ?? (node.filePath ? [node.filePath] : [])
      const activeFile = node.editorActiveFile ?? files[0] ?? node.filePath ?? ''
      pane = {
        id: paneId,
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
    } else if (node.toolId === 'browser') {
      const browserId = crypto.randomUUID()
      pane = {
        id: paneId,
        sessionId: browserId,
        wsUrl: '',
        toolId: node.toolId,
        toolName: node.toolName,
        isRunning: true,
        exitCode: null,
        title: null,
        paneType: 'browser',
        url: node.browserUrl,
      }
    } else if (SDK_TOOL_IDS.has(node.toolId)) {
      const meta = sdkToolMeta(node.toolId)
      const project = getProjectForWorktree(worktreePath)
      const workspaceId = node.workspaceId ?? project?.workspace.id
      const profileId = node.profileId ?? getProfilesByAgent(meta.toolId)[0]?.id
      const existing = !node.conversationId
        ? await findLatestSdkConversation(workspaceId, worktreePath, profileId)
        : null
      let conversationId = node.conversationId ?? existing?.id
      const resolvedProfileId = node.profileId ?? existing?.agentProfileId ?? profileId

      if (!conversationId && workspaceId && resolvedProfileId) {
        const result = await window.api.sdkAgent.create({
          workspaceId,
          worktreePath,
          profileId: resolvedProfileId,
        })
        if (!('error' in result)) conversationId = result.conversationId
      }

      pane = {
        id: paneId,
        sessionId: conversationId ?? crypto.randomUUID(),
        wsUrl: '',
        toolId: meta.toolId,
        toolName: meta.name,
        isRunning: !!conversationId,
        exitCode: conversationId ? null : 1,
        title: null,
        paneType: 'sdkChat',
        conversationId,
        workspaceId,
        profileId: resolvedProfileId,
      }
    } else if (
      node.tmuxSessionName &&
      (await window.api.tmuxHasSession(node.tmuxSessionName).catch(() => false))
    ) {
      // Reattach to an existing tmux session
      const result = await window.api.tmuxAttach(node.tmuxSessionName)
      pane = {
        id: paneId,
        sessionId: result.sessionId,
        wsUrl: result.wsUrl,
        toolId: node.toolId,
        toolName: node.toolName,
        isRunning: true,
        exitCode: null,
        title: null,
        tmuxSessionName: node.tmuxSessionName,
      }
      if (AI_TOOL_IDS.has(node.toolId)) {
        initAgentSession(result.sessionId, node.toolId as AgentType)
      }
    } else {
      const options: {
        workspaceName?: string
        branch?: string
        resumeSessionId?: string
        profileId?: string
      } = {}
      if (AI_TOOL_IDS.has(node.toolId)) {
        const project = getProjectForWorktree(worktreePath)
        options.workspaceName = project?.workspace.name ?? workspaceState.workspace?.name ?? ''
        options.branch = workspaceState.branch ?? undefined
        options.resumeSessionId = node.agentSessionId ?? node.claudeSessionId
        if (node.profileId) options.profileId = node.profileId
      }
      const result = await window.api.spawnTool(node.toolId, worktreePath, options)
      const restoredProfile = node.profileId ? getProfileById(node.profileId) : undefined
      pane = {
        id: paneId,
        sessionId: result.sessionId,
        wsUrl: result.wsUrl,
        toolId: node.toolId,
        toolName: result.toolName,
        isRunning: true,
        exitCode: null,
        title: null,
        tmuxSessionName: result.tmuxSessionName,
        profileId: node.profileId,
        profileName: restoredProfile?.name,
      }
      if (AI_TOOL_IDS.has(node.toolId)) {
        initAgentSession(result.sessionId, node.toolId as AgentType)
      }
    }
    return createLeaf(pane)
  }

  const [first, second] = await Promise.all([
    restoreSplitNode(node.first, worktreePath),
    restoreSplitNode(node.second, worktreePath),
  ])
  return {
    type: node.type,
    id: `split-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    first,
    second,
    ratio: node.ratio,
  }
}

export async function restoreLayout(worktreePath: string, layoutJson: string): Promise<boolean> {
  let layout: SerializedLayout
  try {
    layout = JSON.parse(layoutJson)
  } catch {
    return false
  }

  if (!layout.tabs || layout.tabs.length === 0) return false

  const activeIdx = Math.min(Math.max(0, layout.activeTabIndex), layout.tabs.length - 1)
  const restoredTabs: TabInfo[] = []

  for (let i = 0; i < layout.tabs.length; i++) {
    const serializedTab = layout.tabs[i]

    if (i === activeIdx) {
      // Fully restore the active tab
      try {
        const rootSplit = await restoreSplitNode(serializedTab.rootSplit, worktreePath)
        const id = nextTabId()
        const name = computeDisplayName(serializedTab.toolName, worktreePath, serializedTab.toolId)
        const firstPane = firstLeaf(rootSplit)

        restoredTabs.push({
          id,
          toolId: serializedTab.toolId,
          toolName: serializedTab.toolName,
          name,
          worktreePath,
          rootSplit,
          focusedPaneId: firstPane.id,
        })
      } catch {
        // Active tab failed; create as suspended instead
        const id = nextTabId()
        const name = computeDisplayName(serializedTab.toolName, worktreePath, serializedTab.toolId)
        const placeholderPaneId = nextPaneId()
        restoredTabs.push({
          id,
          toolId: serializedTab.toolId,
          toolName: serializedTab.toolName,
          name,
          worktreePath,
          rootSplit: createLeaf({
            id: placeholderPaneId,
            sessionId: '',
            wsUrl: '',
            toolId: serializedTab.toolId,
            toolName: serializedTab.toolName,
            isRunning: false,
            exitCode: null,
            title: null,
          }),
          focusedPaneId: placeholderPaneId,
          suspended: serializedTab.rootSplit,
        })
      }
    } else {
      // Create suspended tab with placeholder
      const id = nextTabId()
      const name = computeDisplayName(serializedTab.toolName, worktreePath, serializedTab.toolId)
      const placeholderPaneId = nextPaneId()
      restoredTabs.push({
        id,
        toolId: serializedTab.toolId,
        toolName: serializedTab.toolName,
        name,
        worktreePath,
        rootSplit: createLeaf({
          id: placeholderPaneId,
          sessionId: '',
          wsUrl: '',
          toolId: serializedTab.toolId,
          toolName: serializedTab.toolName,
          isRunning: false,
          exitCode: null,
          title: null,
        }),
        focusedPaneId: placeholderPaneId,
        suspended: serializedTab.rootSplit,
      })
    }
  }

  if (restoredTabs.length === 0) return false

  tabsByWorktree[worktreePath] = restoredTabs

  // Prefer a non-suspended tab as active; fall back to first tab
  const activeTab = restoredTabs.find((t) => !t.suspended) ?? restoredTabs[0]
  activeTabId[worktreePath] = activeTab.id

  return true
}
