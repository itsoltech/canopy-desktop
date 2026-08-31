import { match } from 'ts-pattern'
import {
  type PaneSession,
  type SplitNode,
  type EditorFileState,
  createLeaf,
  allPanes,
  findLeaf,
} from './splitTree'
import type { DropZone } from './dragState.svelte'
import { recordFileOpen } from './quickOpenMru.svelte'
import { workspaceState, getProjectForWorktree, selectWorktree } from './workspace.svelte'
import {
  initAgentSession,
  rekeyAgentSession,
  removeAgentSession,
  agentSessions,
  type AgentType,
} from '../agents/agentState.svelte'
import { confirm } from './dialogs.svelte'
import { browserSessions } from '../browser/browserState.svelte'
import { notesUiScope } from './notes.svelte'
import { drawingsState } from './drawings.svelte'
import type {
  EditorFileLoadResult,
  EditorFileSaveResult,
  PaneSnapshot,
  SplitSnapshot,
  TabCloseAllPreflightResult,
  TabClosePreflightResult,
  TabCommandResult,
  TabSnapshot,
  TabStateSnapshot,
} from '../../../../main/commands/types'

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
  delete pendingEditorJumps[pane.id]
}

// --- Active process detection ---

// Typed as Set<AgentType> so a typo or a newly added agent that is not mirrored
// here fails to compile, and widened to ReadonlySet<string> so `has()` accepts
// the arbitrary tool ids stored on panes. That combination makes the type
// predicate below sound without an assertion at the call sites.
const AI_TOOL_IDS: ReadonlySet<string> = new Set<AgentType>([
  'claude',
  'codex',
  'opencode',
  'gemini',
])
export const isAiToolId = (id: string): id is AgentType => AI_TOOL_IDS.has(id)

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

export const tabsByWorktree: Record<string, TabInfo[]> = $state({})
export const activeTabId: Record<string, string> = $state({})
const EMPTY_TABS: TabInfo[] = []

function editorFilesFromSnapshot(
  snapshotFiles: PaneSnapshot['editorFiles'],
  previousFiles: EditorFileState[] | undefined,
): EditorFileState[] | undefined {
  if (!snapshotFiles) return previousFiles?.map((file) => ({ ...file }))

  const previousByPath = new Map(previousFiles?.map((file) => [file.filePath, file]) ?? [])
  return snapshotFiles.map((file) => {
    const previous = previousByPath.get(file.filePath)
    return previous ? { ...file, ...previous, filePath: file.filePath } : { ...file }
  })
}

function paneFromSnapshot(snapshot: PaneSnapshot, previous?: PaneSession): PaneSession {
  if (
    previous &&
    previous.sessionId !== snapshot.sessionId &&
    isAiToolId(snapshot.toolId) &&
    snapshot.isRunning
  ) {
    rekeyAgentSession(previous.sessionId, snapshot.sessionId, snapshot.toolId)
  }

  return {
    ...previous,
    id: snapshot.id,
    sessionId: snapshot.sessionId,
    wsUrl: snapshot.wsUrl,
    toolId: snapshot.toolId,
    toolName: snapshot.toolName,
    isRunning: snapshot.isRunning,
    exitCode: snapshot.exitCode,
    title: snapshot.title,
    paneType: snapshot.paneType,
    filePath: snapshot.filePath ?? previous?.filePath,
    url: snapshot.url,
    tmuxSessionName: snapshot.tmuxSessionName,
    detached: snapshot.detached,
    inspectorOpen: snapshot.inspectorOpen,
    profileId: snapshot.profileId,
    profileName: snapshot.profileName,
    editorFiles: editorFilesFromSnapshot(snapshot.editorFiles, previous?.editorFiles),
    editorActiveFile: snapshot.editorActiveFile ?? previous?.editorActiveFile,
  }
}

function previousPaneFromSplit(
  previous: SplitNode | undefined,
  paneId: string,
): PaneSession | undefined {
  return previous ? allPanes(previous).find((pane) => pane.id === paneId) : undefined
}

function splitFromSnapshot(snapshot: SplitSnapshot, previous?: SplitNode): SplitNode {
  if (snapshot.type === 'leaf') {
    return createLeaf(
      paneFromSnapshot(snapshot.pane, previousPaneFromSplit(previous, snapshot.pane.id)),
    )
  }
  return {
    type: snapshot.direction === 'horizontal' ? 'hsplit' : 'vsplit',
    id: snapshot.id,
    ratio: snapshot.ratio,
    first: splitFromSnapshot(snapshot.first, previous),
    second: splitFromSnapshot(snapshot.second, previous),
  }
}

function tabFromSnapshot(snapshot: TabSnapshot, previous?: TabInfo): TabInfo {
  return {
    id: snapshot.id,
    toolId: snapshot.toolId,
    toolName: snapshot.toolName,
    name: snapshot.name,
    worktreePath: snapshot.worktreePath,
    rootSplit: splitFromSnapshot(snapshot.rootSplit, previous?.rootSplit),
    focusedPaneId: snapshot.focusedPaneId,
    suspended: snapshot.suspended,
  }
}

export function applyTabsSnapshot(
  snapshot: TabStateSnapshot,
  options: { replaceAll?: boolean } = {},
): void {
  if (options.replaceAll) {
    const incomingWorktrees = Object.keys(snapshot.tabsByWorktree)
    for (const worktreePath of Object.keys(tabsByWorktree)) {
      if (!incomingWorktrees.includes(worktreePath)) delete tabsByWorktree[worktreePath]
    }
    const incomingActiveWorktrees = Object.keys(snapshot.activeTabIdByWorktree)
    for (const worktreePath of Object.keys(activeTabId)) {
      if (!incomingActiveWorktrees.includes(worktreePath)) delete activeTabId[worktreePath]
    }
  }

  for (const [worktreePath, tabSnapshots] of Object.entries(snapshot.tabsByWorktree)) {
    const previousTabs = tabsByWorktree[worktreePath] ?? []
    const incomingActiveTabId = snapshot.activeTabIdByWorktree[worktreePath] ?? null

    tabsByWorktree[worktreePath] = tabSnapshots.map((tab) =>
      tabFromSnapshot(
        tab,
        previousTabs.find((previous) => previous.id === tab.id),
      ),
    )
    for (const tab of tabsByWorktree[worktreePath]) {
      for (const pane of allPanes(tab.rootSplit)) initPaneRuntimeState(pane)
    }

    if (incomingActiveTabId) {
      activeTabId[worktreePath] = incomingActiveTabId
    } else {
      delete activeTabId[worktreePath]
    }
  }
}

function initPaneRuntimeState(pane: PaneSession): void {
  if (isAiToolId(pane.toolId) && pane.isRunning) {
    initAgentSession(pane.sessionId, pane.toolId)
  }
}

function applyTabCommandResult(result: TabCommandResult): void {
  applyTabsSnapshot({
    tabsByWorktree: { [result.worktreePath]: result.tabs },
    activeTabIdByWorktree: { [result.worktreePath]: result.activeTabId },
  })
}

function applyOpenedTabResult(result: TabCommandResult): TabInfo | null {
  const snapshot = result.openedTab
  if (!snapshot) return null
  applyTabCommandResult(result)
  return tabsByWorktree[result.worktreePath]?.find((tab) => tab.id === snapshot.id) ?? null
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
  const project = getProjectForWorktree(worktreePath)
  const result = await window.api.tabOpenTool(toolId, worktreePath, {
    initialUrl: options?.initialUrl,
    profileId: options?.profileId,
    workspaceName: project?.workspace.name ?? workspaceState.workspace?.name ?? '',
    branch: workspaceState.branch ?? undefined,
  })
  const tab = applyOpenedTabResult(result)
  if (!tab) throw new Error('tab:command:openTool did not return an opened tab')

  scheduleSave(worktreePath)
  return tab
}

export function openTmuxTab(
  _tmuxSessionName: string,
  sessionId: string,
  worktreePath: string,
): void {
  void openSessionTabInMain('Shell', sessionId, worktreePath).catch((err) => {
    console.error(`[tabs] tabOpenSessionTab failed for "${worktreePath}":`, err)
  })
}

export function openRunConfigTab(
  configName: string,
  sessionId: string,
  worktreePath: string,
): void {
  void openSessionTabInMain(configName, sessionId, worktreePath).catch((err) => {
    console.error(`[tabs] tabOpenSessionTab failed for "${worktreePath}":`, err)
  })
}

async function openSessionTabInMain(
  name: string,
  sessionId: string,
  worktreePath: string,
): Promise<TabInfo | null> {
  const result = await window.api.tabOpenSessionTab(worktreePath, name, sessionId)
  const tab = applyOpenedTabResult(result)
  if (tab) scheduleSave(worktreePath)
  return tab
}

async function handleClosePreflightFailure(
  preflight: TabClosePreflightResult,
  cancelledMessage: string,
): Promise<boolean> {
  if (preflight.ok) return true
  if (preflight.reason === 'save-failed') {
    await confirm({
      title: 'Save failed',
      message: `Could not save ${preflight.failedCount} file(s). ${cancelledMessage}`,
      confirmLabel: 'OK',
    })
  }
  return false
}

async function handleCloseAllPreflightFailure(
  preflight: TabCloseAllPreflightResult,
  cancelledMessage: string,
): Promise<boolean> {
  if (!preflight.ok && preflight.reason === 'active-processes') return false
  return handleClosePreflightFailure(preflight, cancelledMessage)
}

async function prepareCloseAllTabsForWorktree(
  worktreePath: string,
  tabs: TabInfo[],
): Promise<boolean> {
  let preflight = await window.api.tabPrepareCloseAllForWorktree(worktreePath)

  if (!preflight.ok && preflight.reason === 'active-processes') {
    const confirmed = await confirm({
      title: tabs.length === 1 ? 'Close tab?' : 'Close all tabs?',
      message:
        preflight.warnings.length === 1
          ? `A tab has ${preflight.warnings[0].description} that will be terminated.`
          : `${preflight.warnings.length} tabs have active processes that will be terminated.`,
      details: preflight.warnings
        .map((warning) => `${warning.tabName}: ${warning.description}`)
        .join('\n'),
      confirmLabel: tabs.length === 1 ? 'Close Tab' : 'Close All Tabs',
      destructive: true,
    })
    if (!confirmed) return false

    preflight = await window.api.tabPrepareCloseAllForWorktree(worktreePath, {
      confirmedActiveProcesses: true,
    })
  }

  return handleCloseAllPreflightFailure(preflight, 'Close all tabs cancelled.')
}

export async function closeTab(tabId: string): Promise<void> {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    const idx = tabs.findIndex((t) => t.id === tabId)
    if (idx === -1) continue

    const tab = tabs[idx]

    if (tab.suspended) {
      const result = await window.api.tabCloseTab(path, tabId)

      applyTabCommandResult(result)
      scheduleSave(path)
      return
    }

    const panes = allPanes(tab.rootSplit)

    const closePreflight = await window.api.tabPrepareCloseTab(path, tabId)
    if (!(await handleClosePreflightFailure(closePreflight, 'Tab close cancelled.'))) {
      return
    }

    const { description } = await window.api.tabGetCloseWarning(path, {
      kind: 'tab',
      tabId,
    })
    if (description) {
      const confirmed = await confirm({
        title: 'Close tab?',
        message: `This tab has ${description} that will be terminated.`,
        confirmLabel: 'Close Tab',
        destructive: true,
      })
      if (!confirmed) return
    }

    // Editor dirty state lives in the renderer, so the main-process close
    // warning above cannot see it — guard unsaved buffers separately.
    if (isTabDirty(tab)) {
      const confirmed = await confirm({
        title: 'Close tab?',
        message: 'This tab has unsaved changes that will be lost.',
        confirmLabel: 'Discard & Close',
        destructive: true,
      })
      if (!confirmed) return
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
    const result = await window.api.tabCloseTab(path, tabId)

    applyTabCommandResult(result)
    scheduleSave(path)
    return
  }
}

export async function switchTab(tabId: string): Promise<void> {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    const tab = tabs.find((t) => t.id === tabId)
    if (tab) {
      if (tab.suspended && !(await resumeTab(tab))) return
      await setActiveTabInMain(path, tabId)
      return
    }
  }
}

export function moveTab(worktreePath: string, fromIndex: number, toIndex: number): void {
  void moveTabInMain(worktreePath, fromIndex, toIndex).catch((err) => {
    console.error(`[tabs] tabMoveTab failed for "${worktreePath}":`, err)
  })
}

async function moveTabInMain(
  worktreePath: string,
  fromIndex: number,
  toIndex: number,
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || fromIndex === toIndex) return
  if (fromIndex < 0 || fromIndex >= tabs.length) return
  if (toIndex < 0 || toIndex >= tabs.length) return

  const result = await window.api.tabMoveTab(worktreePath, fromIndex, toIndex)
  applyTabCommandResult(result)
  scheduleSave(worktreePath)
}

export async function switchTabByIndex(worktreePath: string, index: number): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (tabs && index >= 0 && index < tabs.length) {
    const tab = tabs[index]
    if (tab.suspended && !(await resumeTab(tab))) return
    await setActiveTabInMain(worktreePath, tab.id)
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
  await setActiveTabInMain(worktreePath, tab.id)
}

export async function prevTab(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || tabs.length <= 1) return

  const currentId = activeTabId[worktreePath]
  const idx = tabs.findIndex((t) => t.id === currentId)
  const prevIdx = (idx - 1 + tabs.length) % tabs.length
  const tab = tabs[prevIdx]
  if (tab.suspended && !(await resumeTab(tab))) return
  await setActiveTabInMain(worktreePath, tab.id)
}

async function setActiveTabInMain(worktreePath: string, tabId: string): Promise<void> {
  const result = await window.api.tabSetActiveTab(worktreePath, tabId)
  applyTabCommandResult(result)
  if (result.activeTabId === tabId) scheduleSave(worktreePath)
}

export async function reopenClosedTab(worktreePath: string): Promise<void> {
  const project = getProjectForWorktree(worktreePath)
  const result = await window.api.tabReopenClosedTab(worktreePath, {
    workspaceName: project?.workspace.name ?? workspaceState.workspace?.name ?? '',
    branch: workspaceState.branch ?? undefined,
  })
  if (!result.openedTab) return
  applyTabCommandResult(result)
  scheduleSave(worktreePath)
}

export const pendingEditorJumps = $state<Record<string, number>>({})

export function openFile(filePath: string, worktreePath: string, opts?: { line?: number }): void {
  const relPath = filePath.startsWith(worktreePath + '/')
    ? filePath.slice(worktreePath.length + 1)
    : filePath
  recordFileOpen(worktreePath, relPath)

  void openFileInMain(filePath, worktreePath, opts).catch((err) => {
    console.error(`[tabs] tabOpenEditorFile failed for "${worktreePath}":`, err)
  })
}

async function openFileInMain(
  filePath: string,
  worktreePath: string,
  opts?: { line?: number },
): Promise<void> {
  const result = await window.api.tabOpenEditorFile(worktreePath, filePath)

  applyTabCommandResult(result)
  const active = result.tabs.find((tab) => tab.id === result.activeTabId)
  if (opts?.line && active) {
    pendingEditorJumps[active.focusedPaneId] = opts.line
  }
  scheduleSave(worktreePath)
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

  void moveEditorFileBetweenPanesInMain(sourcePaneId, targetPaneId, filePath, toIndex).catch(
    (err) => {
      console.error(`[tabs] tabMoveEditorFileBetweenPanes failed for pane "${sourcePaneId}":`, err)
    },
  )
}

async function moveEditorFileBetweenPanesInMain(
  sourcePaneId: string,
  targetPaneId: string,
  filePath: string,
  toIndex: number,
): Promise<void> {
  let sourceWorktree: string | null = null
  let targetWorktree: string | null = null
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const pane = findLeaf(tab.rootSplit, sourcePaneId)
      if (pane?.editorFiles?.some((file) => file.filePath === filePath)) {
        sourceWorktree = worktreePath
      }
      if (findLeaf(tab.rootSplit, targetPaneId)) {
        targetWorktree = worktreePath
      }
    }
  }
  if (!sourceWorktree || !targetWorktree || sourceWorktree !== targetWorktree) return

  const result = await window.api.tabMoveEditorFileBetweenPanes(
    targetWorktree,
    sourcePaneId,
    targetPaneId,
    filePath,
    toIndex,
  )
  applyTabCommandResult(result)
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
  void moveEditorFileInMain(paneId, filePath, toIndex).catch((err) => {
    console.error(`[tabs] tabMoveEditorFile failed for pane "${paneId}":`, err)
  })
}

async function moveEditorFileInMain(
  paneId: string,
  filePath: string,
  toIndex: number,
): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      const files = existing.editorFiles ?? []
      if (!files.some((f) => f.filePath === filePath)) continue

      const result = await window.api.tabMoveEditorFile(worktreePath, paneId, filePath, toIndex)
      applyTabCommandResult(result)
      scheduleSave(worktreePath)
      return
    }
  }
}

export function setActiveEditorFile(paneId: string, filePath: string): void {
  void setActiveEditorFileInMain(paneId, filePath).catch((err) => {
    console.error(`[tabs] tabSetActiveEditorFile failed for pane "${paneId}":`, err)
  })
}

async function setActiveEditorFileInMain(paneId: string, filePath: string): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue

      const result = await window.api.tabSetActiveEditorFile(worktreePath, paneId, filePath)
      applyTabCommandResult(result)
      return
    }
  }
}

export function updateEditorFileState(
  paneId: string,
  filePath: string,
  patch: Partial<EditorFileState>,
): void {
  void updateEditorFileStateInMain(paneId, filePath, patch).catch((err) => {
    console.error(`[tabs] tabUpdateEditorFileState failed for pane "${paneId}":`, err)
  })
}

async function updateEditorFileStateInMain(
  paneId: string,
  filePath: string,
  patch: Partial<EditorFileState>,
): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue

      const result = await window.api.tabUpdateEditorFileState(
        worktreePath,
        paneId,
        filePath,
        patch,
      )
      applyTabCommandResult(result)
      return
    }
  }
}

export async function loadEditorFile(
  paneId: string,
  filePath: string,
  maxBytes?: number,
): Promise<EditorFileLoadResult> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue

      const result = await window.api.tabLoadEditorFile(worktreePath, paneId, filePath, {
        maxBytes,
      })
      if (result.ok) applyTabCommandResult(result.result)
      return result
    }
  }
  return { ok: false, tag: 'ReadFailed', message: 'Editor file is not open' }
}

export async function saveEditorFile(
  paneId: string,
  filePath: string,
  content: string,
  fileLineEnding: 'LF' | 'CRLF',
  expectedMtimeMs?: number,
): Promise<EditorFileSaveResult> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue

      const result = await window.api.tabSaveEditorFile(worktreePath, paneId, filePath, {
        content,
        fileLineEnding,
        expectedMtimeMs,
      })
      if (result.ok) applyTabCommandResult(result.result)
      return result
    }
  }
  return { ok: false, tag: 'WriteFailed', message: 'Editor file is not open' }
}

export async function prepareCloseEditorFile(
  paneId: string,
  filePath: string,
): Promise<TabClosePreflightResult> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue
      return window.api.tabPrepareCloseEditorFile(worktreePath, paneId, filePath)
    }
  }
  return { ok: true }
}

export function closeEditorFile(paneId: string, filePath: string): void {
  void closeEditorFileInMain(paneId, filePath).catch((err) => {
    console.error(`[tabs] tabCloseEditorFile failed for pane "${paneId}":`, err)
  })
}

async function closeEditorFileInMain(paneId: string, filePath: string): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue

      const result = await window.api.tabCloseEditorFile(worktreePath, paneId, filePath)
      applyTabCommandResult(result)
      if (result.closedPaneId) scheduleSave(worktreePath)
      return
    }
  }
}

export function detachEditorFile(paneId: string, filePath: string): void {
  void detachEditorFileInMain(paneId, filePath).catch((err) => {
    console.error(`[tabs] tabDetachEditorFile failed for pane "${paneId}":`, err)
  })
}

async function detachEditorFileInMain(paneId: string, filePath: string): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const existing = findLeaf(tab.rootSplit, paneId)
      if (!existing) continue
      if (!existing.editorFiles?.some((f) => f.filePath === filePath)) continue

      const result = await window.api.tabDetachEditorFile(worktreePath, paneId, filePath)
      applyTabCommandResult(result)
      if (result.openedTab) scheduleSave(worktreePath)
      return
    }
  }
}

export function openDiffTab(worktreePath: string, scrollToFile?: string): void {
  void openDiffTabInMain(worktreePath, scrollToFile).catch((err) => {
    console.error(`[tabs] tabOpenDiff failed for "${worktreePath}":`, err)
  })
}

async function openDiffTabInMain(worktreePath: string, scrollToFile?: string): Promise<void> {
  const result = await window.api.tabOpenDiff(worktreePath)
  applyTabCommandResult(result)

  if (scrollToFile) {
    workspaceState.diffScrollTarget = { path: scrollToFile, ts: Date.now() }
  }
  if (result.openedTab) scheduleSave(worktreePath)
}

export function getActiveTab(worktreePath: string): TabInfo | null {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return null
  const id = activeTabId[worktreePath]
  return tabs.find((t) => t.id === id) ?? null
}

export function getTabsForWorktree(worktreePath: string): TabInfo[] {
  return tabsByWorktree[worktreePath] ?? EMPTY_TABS
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
        const result = await window.api.tabHandlePtyExit(
          worktreePath,
          sessionId,
          exitCode,
          tmuxSessionName,
        )
        applyTabCommandResult(result)
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

  const project = getProjectForWorktree(worktreePath)
  const result = await window.api.tabRestartPane(worktreePath, tabId, paneId, {
    workspaceName: project?.workspace.name ?? workspaceState.workspace?.name ?? '',
    branch: workspaceState.branch ?? undefined,
  })
  if (!result.restartedPane) return

  if (AI_TOOL_IDS.has(pane.toolId) && pane.sessionId !== result.restartedPane.sessionId) {
    removeAgentSession(pane.sessionId)
  }
  if (pane.paneType === 'browser' && pane.sessionId !== result.restartedPane.sessionId) {
    delete browserSessions[pane.sessionId]
  }

  applyTabCommandResult(result)
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

  const project = getProjectForWorktree(worktreePath)
  const result = await window.api.tabReattachTmuxPane(worktreePath, tabId, paneId, {
    workspaceName: project?.workspace.name ?? workspaceState.workspace?.name ?? '',
    branch: workspaceState.branch ?? undefined,
  })
  applyTabCommandResult(result)
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

  const result = await window.api.tabKillTmuxPane(worktreePath, tabId, paneId)
  applyTabCommandResult(result)
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

async function resumeTab(tab: TabInfo): Promise<boolean> {
  if (!tab.suspended) return true
  try {
    const project = getProjectForWorktree(tab.worktreePath)
    const result = await window.api.tabResumeSuspendedTab(tab.worktreePath, tab.id, {
      workspaceName: project?.workspace.name ?? workspaceState.workspace?.name ?? '',
      branch: workspaceState.branch ?? undefined,
    })
    const resumed = result.tabs.find((candidate) => candidate.id === tab.id)
    if (resumed?.suspended) return false
    applyTabCommandResult(result)
    return true
  } catch (err) {
    console.error('Failed to resume suspended tab:', err)
    return false
  }
}

export async function closeAllTabsForWorktree(
  worktreePath: string,
  options?: { forRemoval?: boolean },
): Promise<boolean> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || tabs.length === 0) return true

  if (!(await prepareCloseAllTabsForWorktree(worktreePath, [...tabs]))) return false

  const currentTabs = tabsByWorktree[worktreePath] ?? []
  const allSessions = currentTabs.filter((t) => !t.suspended).flatMap((t) => allPanes(t.rootSplit))
  for (const p of allSessions) {
    if (agentSessions[p.sessionId]) removeAgentSession(p.sessionId)
    if (p.paneType === 'browser') delete browserSessions[p.sessionId]
    disposeEphemeralPaneState(p)
  }

  const result = await window.api.tabCloseAllForWorktree(worktreePath, options?.forRemoval)

  if (saveTimers[worktreePath]) {
    clearTimeout(saveTimers[worktreePath])
    delete saveTimers[worktreePath]
  }
  applyTabCommandResult(result)
  return true
}

export async function killAllTabs(): Promise<void> {
  const allTabsList = Object.values(tabsByWorktree).flat()
  const allSessions = allTabsList.filter((t) => !t.suspended).flatMap((t) => allPanes(t.rootSplit))
  for (const p of allSessions) disposeEphemeralPaneState(p)
  const result = await window.api.tabKillAll()
  applyTabsSnapshot(result, { replaceAll: true })
}

export function focusSessionByPtyId(ptySessionId: string): void {
  void focusSessionByPtyIdInMain(ptySessionId).catch((err) => {
    console.error(`[tabs] tabFocusSession failed for "${ptySessionId}":`, err)
  })
}

async function focusSessionByPtyIdInMain(ptySessionId: string): Promise<void> {
  const result = await window.api.tabFocusSession(ptySessionId)
  if (!result) return

  // Use selectWorktree to fully update project context (sidebar, git info, etc.)
  await selectWorktree(result.worktreePath).catch((err) => {
    console.error('[tabs] selectWorktree failed after focusSession:', err)
  })
  applyTabCommandResult(result)
}

export function getAllTabs(): TabInfo[] {
  const groups = Object.values(tabsByWorktree)
  if (groups.length === 0) return EMPTY_TABS
  if (groups.length === 1) return groups[0] ?? EMPTY_TABS

  let count = 0
  for (const group of groups) count += group.length
  if (count === 0) return EMPTY_TABS

  return groups.flat()
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
    void toggleFocusedInspectorInMain(path, tab.id).catch((err) => {
      console.error(`[tabs] tabToggleFocusedInspector failed for tab "${tab.id}":`, err)
    })
  }
}

async function toggleFocusedInspectorInMain(worktreePath: string, tabId: string): Promise<void> {
  const result = await window.api.tabToggleFocusedInspector(worktreePath, tabId)
  applyTabCommandResult(result)
}

export function updateTmuxSessionName(oldName: string, newName: string): void {
  void updateTmuxSessionNameInMain(oldName, newName).catch((err) => {
    console.error(`[tabs] tabUpdateTmuxSessionName failed for "${oldName}":`, err)
  })
}

async function updateTmuxSessionNameInMain(oldName: string, newName: string): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const pane = panes.find((p) => p.tmuxSessionName === oldName)
      if (pane) {
        const result = await window.api.tabUpdateTmuxSessionName(worktreePath, oldName, newName)
        applyTabCommandResult(result)
        scheduleSave(worktreePath)
        return
      }
    }
  }
}

export function updatePaneTitle(sessionId: string, title: string): void {
  if (!title) return
  void updatePaneTitleInMain(sessionId, title).catch((err) => {
    console.error(`[tabs] tabUpdatePaneTitle failed for session "${sessionId}":`, err)
  })
}

async function updatePaneTitleInMain(sessionId: string, title: string): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const pane = panes.find((p) => p.sessionId === sessionId)
      if (pane) {
        const result = await window.api.tabUpdatePaneTitle(worktreePath, sessionId, title)
        applyTabCommandResult(result)
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
  void updateBrowserPaneUrlInMain(sessionId, url).catch((err) => {
    console.error(`[tabs] tabUpdatePaneUrl failed for session "${sessionId}":`, err)
  })
}

async function updateBrowserPaneUrlInMain(sessionId: string, url: string): Promise<void> {
  for (const [worktreePath, tabs] of Object.entries(tabsByWorktree)) {
    for (const tab of tabs) {
      const panes = allPanes(tab.rootSplit)
      const pane = panes.find((p) => p.sessionId === sessionId)
      if (pane) {
        const result = await window.api.tabUpdatePaneUrl(worktreePath, sessionId, url)
        applyTabCommandResult(result)
        scheduleSave(worktreePath)
        return
      }
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

// --- Split pane operations ---

export async function splitFocusedPane(
  worktreePath: string,
  direction: 'hsplit' | 'vsplit',
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const tabId = activeTabId[worktreePath]
  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return

  const result = await window.api.tabSplitPane(
    worktreePath,
    tab.id,
    tab.focusedPaneId,
    direction === 'hsplit' ? 'horizontal' : 'vertical',
  )
  applyTabCommandResult(result)
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

  const { description } = await window.api.tabGetCloseWarning(worktreePath, {
    kind: 'pane',
    tabId,
    paneId,
  })
  if (description) {
    const confirmed = await confirm({
      title: 'Close pane?',
      message: `This pane has ${description} that will be terminated.`,
      confirmLabel: 'Close Pane',
      destructive: true,
    })
    if (!confirmed) return
  }

  const result = await window.api.tabClosePane(worktreePath, tabId, paneId)
  if (!result.closedPaneId) return

  if (agentSessions[pane.sessionId]) removeAgentSession(pane.sessionId)
  if (pane.paneType === 'browser') delete browserSessions[pane.sessionId]
  disposeEphemeralPaneState(pane)

  applyTabCommandResult(result)
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
  void navigatePaneFocusInMain(worktreePath, direction).catch((err) => {
    console.error(`[tabs] tabNavigatePaneFocus failed for "${worktreePath}":`, err)
  })
}

async function navigatePaneFocusInMain(
  worktreePath: string,
  direction: 'left' | 'right' | 'up' | 'down',
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const tabId = activeTabId[worktreePath]
  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return

  const result = await window.api.tabNavigatePaneFocus(worktreePath, tab.id, direction)
  applyTabCommandResult(result)
  if (
    result.tabs.some(
      (candidate) => candidate.id === tab.id && candidate.focusedPaneId !== tab.focusedPaneId,
    )
  ) {
    scheduleSave(worktreePath)
  }
}

export function focusPane(worktreePath: string, tabId: string, paneId: string): void {
  void focusPaneInMain(worktreePath, tabId, paneId).catch((err) => {
    console.error(`[tabs] tabFocusPane failed for "${worktreePath}":`, err)
  })
}

async function focusPaneInMain(worktreePath: string, tabId: string, paneId: string): Promise<void> {
  const result = await window.api.tabFocusPane(worktreePath, tabId, paneId)
  applyTabCommandResult(result)
  if (result.tabs.some((tab) => tab.id === tabId && tab.focusedPaneId === paneId)) {
    scheduleSave(worktreePath)
  }
}

export function updateSplitRatio(
  worktreePath: string,
  tabId: string,
  splitId: string,
  ratio: number,
): void {
  void updateSplitRatioInMain(worktreePath, tabId, splitId, ratio).catch((err) => {
    console.error(`[tabs] tabUpdateSplitRatio failed for "${worktreePath}":`, err)
  })
}

async function updateSplitRatioInMain(
  worktreePath: string,
  tabId: string,
  splitId: string,
  ratio: number,
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs?.some((tab) => tab.id === tabId)) return

  const result = await window.api.tabUpdateSplitRatio(worktreePath, tabId, splitId, ratio)
  applyTabCommandResult(result)
  if (
    result.tabs.some(
      (tab) =>
        tab.id === tabId &&
        tab.rootSplit.type === 'split' &&
        tab.rootSplit.id === splitId &&
        tab.rootSplit.ratio === ratio,
    )
  ) {
    scheduleSave(worktreePath)
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
  const result = await window.api.tabMoveTabToSplit(
    worktreePath,
    sourceTabId,
    targetTabId,
    targetPaneId,
    direction === 'hsplit' ? 'horizontal' : 'vertical',
    position,
  )

  const moved = !result.tabs.some((tab) => tab.id === sourceTabId)
  applyTabCommandResult(result)
  if (moved) scheduleSave(worktreePath)
  return moved
}

// --- Move pane within or across tabs ---

export function movePaneToTarget(
  worktreePath: string,
  sourceTabId: string,
  sourcePaneId: string,
  targetTabId: string,
  targetPaneId: string,
  zone: DropZone,
): void {
  void movePaneToTargetInMain(
    worktreePath,
    sourceTabId,
    sourcePaneId,
    targetTabId,
    targetPaneId,
    zone,
  ).catch((err) => {
    console.error(`[tabs] tabMovePaneToTarget failed for "${worktreePath}":`, err)
  })
}

async function movePaneToTargetInMain(
  worktreePath: string,
  sourceTabId: string,
  sourcePaneId: string,
  targetTabId: string,
  targetPaneId: string,
  zone: DropZone,
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const sourceTab = tabs.find((t) => t.id === sourceTabId)
  const targetTab = tabs.find((t) => t.id === targetTabId)
  if (!sourceTab || !targetTab) return

  const { direction, position } = mapZone(zone)
  const result = await window.api.tabMovePaneToTarget(
    worktreePath,
    sourceTabId,
    sourcePaneId,
    targetTabId,
    targetPaneId,
    direction === 'hsplit' ? 'horizontal' : 'vertical',
    position,
  )

  const moved = result.tabs.some((tab) => tab.focusedPaneId === sourcePaneId)
  applyTabCommandResult(result)
  if (moved) scheduleSave(worktreePath)
}

export function detachPaneToTab(
  worktreePath: string,
  sourceTabId: string,
  sourcePaneId: string,
): void {
  void detachPaneToTabInMain(worktreePath, sourceTabId, sourcePaneId).catch((err) => {
    console.error(`[tabs] tabDetachPaneToTab failed for "${worktreePath}":`, err)
  })
}

async function detachPaneToTabInMain(
  worktreePath: string,
  sourceTabId: string,
  sourcePaneId: string,
): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const sourceTab = tabs.find((t) => t.id === sourceTabId)
  if (!sourceTab) return

  // Already a standalone tab — nothing to detach
  if (sourceTab.rootSplit.type === 'leaf') return

  const result = await window.api.tabDetachPaneToTab(worktreePath, sourceTabId, sourcePaneId)
  applyTabCommandResult(result)
  if (result.openedTab) scheduleSave(worktreePath)
}

// --- Layout persistence ---

const saveTimers: Record<string, ReturnType<typeof setTimeout>> = {}

function scheduleSave(worktreePath: string): void {
  if (saveTimers[worktreePath]) clearTimeout(saveTimers[worktreePath])
  saveTimers[worktreePath] = setTimeout(() => {
    delete saveTimers[worktreePath]
    window.api.tabSaveCurrentLayout(worktreePath).catch(() => {
      // Ignore save errors silently
    })
  }, 500)
}

export function saveAllLayouts(): void {
  for (const path of Object.keys(saveTimers)) {
    clearTimeout(saveTimers[path])
    delete saveTimers[path]
  }
  for (const path of Object.keys(tabsByWorktree)) {
    window.api.tabSaveCurrentLayout(path).catch(() => {
      // Ignore save errors silently
    })
  }
}

export async function restoreLayout(worktreePath: string, layoutJson: string): Promise<boolean> {
  const project = getProjectForWorktree(worktreePath)
  const result = await window.api.tabRestoreLayout(worktreePath, layoutJson, {
    workspaceName: project?.workspace.name ?? workspaceState.workspace?.name ?? '',
    branch: workspaceState.branch ?? undefined,
  })
  if (!result.restored) return false

  applyTabCommandResult(result)
  return true
}
