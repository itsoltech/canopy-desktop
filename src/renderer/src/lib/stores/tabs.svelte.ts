import {
  type PaneSession,
  type SplitNode,
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
import { workspaceState, getProjectForWorktree } from './workspace.svelte'
import {
  initAgentSession,
  removeAgentSession,
  agentSessions,
  type AgentType,
} from '../agents/agentState.svelte'
import { confirm } from './dialogs.svelte'
import { browserSessions } from '../browser/browserState.svelte'

// --- Active process detection ---

const AI_TOOL_IDS = new Set(['claude', 'codex', 'opencode', 'gemini'])
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

  await Promise.all(
    panes.map(async (p) => {
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

  if (busyClaude === 0 && activeShell === 0) return null

  const parts: string[] = []
  if (busyClaude > 0) {
    parts.push(`${busyClaude} active Claude session${busyClaude > 1 ? 's' : ''}`)
  }
  if (activeShell > 0) {
    parts.push(`${activeShell} running process${activeShell > 1 ? 'es' : ''}`)
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
      tmuxSessionName?: string
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

function computeDisplayName(toolName: string, worktreePath: string, toolId: string): string {
  const existing = tabsByWorktree[worktreePath] ?? []
  const sameToolCount = existing.filter((t) => t.toolId === toolId).length
  if (sameToolCount === 0) return toolName
  return `${toolName} #${sameToolCount + 1}`
}

export async function openTool(
  toolId: string,
  worktreePath: string,
  initialUrl?: string,
): Promise<TabInfo> {
  let pane: PaneSession
  const paneId = nextPaneId()
  let toolName: string

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
      url: initialUrl,
    }
  } else {
    const options: { workspaceName?: string; branch?: string } = {}
    if (AI_TOOL_IDS.has(toolId)) {
      const project = getProjectForWorktree(worktreePath)
      options.workspaceName = project?.workspace.name ?? workspaceState.workspace?.name ?? ''
      options.branch = workspaceState.branch ?? undefined
    }
    const result = await window.api.spawnTool(toolId, worktreePath, options)
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
    }
    if (AI_TOOL_IDS.has(toolId)) {
      initAgentSession(result.sessionId, toolId as AgentType)
    }
  }

  const id = nextTabId()
  const name = computeDisplayName(toolName, worktreePath, toolId)

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

export async function closeTab(tabId: string): Promise<void> {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    const idx = tabs.findIndex((t) => t.id === tabId)
    if (idx === -1) continue

    const tab = tabs[idx]

    // Check for active processes before closing
    const panes = allPanes(tab.rootSplit)
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
    }
    await Promise.all(
      panes
        .filter((p) => p.paneType !== 'editor')
        .map((p) => {
          if (p.paneType === 'browser') return window.api.teardownBrowserWebview(p.sessionId)
          // For tmux-backed panes, detach instead of kill (session survives)
          if (p.tmuxSessionName) return window.api.tmuxDetach(p.sessionId).catch(() => {})
          return window.api.killPty(p.sessionId)
        }),
    )

    // Remove tab
    tabsByWorktree[path].splice(idx, 1)

    // If this was the active tab, switch to another
    if (activeTabId[path] === tabId) {
      const remaining = tabsByWorktree[path]
      if (remaining.length > 0) {
        const newIdx = Math.min(idx, remaining.length - 1)
        activeTabId[path] = remaining[newIdx].id
      } else {
        delete activeTabId[path]
      }
    }

    scheduleSave(path)
    return
  }
}

export function switchTab(tabId: string): void {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    if (tabs.some((t) => t.id === tabId)) {
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

export function switchTabByIndex(worktreePath: string, index: number): void {
  const tabs = tabsByWorktree[worktreePath]
  if (tabs && index >= 0 && index < tabs.length) {
    activeTabId[worktreePath] = tabs[index].id
  }
}

export function nextTab(worktreePath: string): void {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || tabs.length <= 1) return

  const currentId = activeTabId[worktreePath]
  const idx = tabs.findIndex((t) => t.id === currentId)
  const nextIdx = (idx + 1) % tabs.length
  activeTabId[worktreePath] = tabs[nextIdx].id
}

export function prevTab(worktreePath: string): void {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || tabs.length <= 1) return

  const currentId = activeTabId[worktreePath]
  const idx = tabs.findIndex((t) => t.id === currentId)
  const prevIdx = (idx - 1 + tabs.length) % tabs.length
  activeTabId[worktreePath] = tabs[prevIdx].id
}

export async function reopenClosedTab(worktreePath: string): Promise<void> {
  const stack = closedTabs[worktreePath]
  if (!stack || stack.length === 0) return

  const entry = stack.pop()!
  await openTool(entry.toolId, worktreePath)
}

export function openFile(filePath: string, worktreePath: string): void {
  // Check if already open - focus it
  const tabs = tabsByWorktree[worktreePath] ?? []
  for (const tab of tabs) {
    const panes = allPanes(tab.rootSplit)
    const existing = panes.find((p) => p.paneType === 'editor' && p.filePath === filePath)
    if (existing) {
      activeTabId[worktreePath] = tab.id
      tab.focusedPaneId = existing.id
      return
    }
  }

  // Create new editor tab
  const paneId = nextPaneId()
  const fileName = filePath.split('/').pop() ?? 'File'
  const pane: PaneSession = {
    id: paneId,
    sessionId: '',
    wsUrl: '',
    toolId: 'editor',
    toolName: fileName,
    isRunning: true,
    exitCode: null,
    title: null,
    paneType: 'editor',
    filePath,
  }

  const id = nextTabId()
  const name = computeDisplayName(fileName, worktreePath, 'editor')

  const tab: TabInfo = {
    id,
    toolId: 'editor',
    toolName: fileName,
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
  const tabs = tabsByWorktree[worktreePath] ?? []
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
  for (const tabs of Object.values(tabsByWorktree)) {
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

  if (pane.paneType === 'editor') {
    // Editor panes have no session to restart. The component re-reads on mount.
    scheduleSave(worktreePath)
    return
  }

  if (pane.paneType === 'browser') {
    const oldUrl = pane.url
    try {
      await window.api.teardownBrowserWebview(pane.sessionId)
    } catch {
      // Already destroyed
    }
    const newBrowserId = crypto.randomUUID()
    tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
      ...p,
      sessionId: newBrowserId,
      url: oldUrl,
      isRunning: true,
      exitCode: null,
      title: null,
    }))
  } else if (pane.tmuxSessionName && pane.detached) {
    // Tmux pane: try to reattach to existing session
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
  } else {
    // Kill old PTY (may already be dead)
    try {
      await window.api.killPty(pane.sessionId)
    } catch {
      // Already exited or cleaned up
    }

    if (AI_TOOL_IDS.has(pane.toolId)) {
      removeAgentSession(pane.sessionId)
    }

    // Spawn new
    const options: { workspaceName?: string; branch?: string } = {}
    if (AI_TOOL_IDS.has(pane.toolId)) {
      options.workspaceName = workspaceState.workspace?.name ?? ''
      options.branch = workspaceState.branch ?? undefined
    }
    const result = await window.api.spawnTool(pane.toolId, worktreePath, options)

    if (AI_TOOL_IDS.has(pane.toolId)) {
      initAgentSession(result.sessionId, pane.toolId as AgentType)
    }

    // Update pane in tree
    tab.rootSplit = treeUpdatePane(tab.rootSplit, paneId, (p) => ({
      ...p,
      sessionId: result.sessionId,
      wsUrl: result.wsUrl,
      isRunning: true,
      exitCode: null,
      title: null,
      tmuxSessionName: result.tmuxSessionName,
    }))
  }

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

    // Restart the focused pane
    await restartPane(tab.worktreePath, tabId, tab.focusedPaneId)
    return
  }
}

export async function ensureShellTab(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (tabs && tabs.length > 0) return
  await openTool('shell', worktreePath)
}

export async function closeAllTabsForWorktree(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs || tabs.length === 0) return

  const allSessions = tabs.flatMap((t) => allPanes(t.rootSplit))
  for (const p of allSessions) {
    if (agentSessions[p.sessionId]) removeAgentSession(p.sessionId)
    if (p.paneType === 'browser') delete browserSessions[p.sessionId]
  }
  await Promise.allSettled(
    allSessions
      .filter((p) => p.paneType !== 'editor')
      .map((p) => {
        if (p.paneType === 'browser') return window.api.teardownBrowserWebview(p.sessionId)
        if (p.tmuxSessionName) return window.api.tmuxDetach(p.sessionId).catch(() => {})
        return window.api.killPty(p.sessionId)
      }),
  )

  delete tabsByWorktree[worktreePath]
  delete activeTabId[worktreePath]

  const wsId = getProjectForWorktree(worktreePath)?.workspace.id ?? workspaceState.workspace?.id
  if (wsId) {
    window.api.deleteLayout(wsId, worktreePath).catch(() => {})
  }
}

export async function killAllTabs(): Promise<void> {
  const allTabsList = Object.values(tabsByWorktree).flat()
  const allSessions = allTabsList.flatMap((t) => allPanes(t.rootSplit))
  await Promise.all(
    allSessions
      .filter((p) => p.paneType !== 'editor')
      .map((p) => {
        if (p.paneType === 'browser') return window.api.teardownBrowserWebview(p.sessionId)
        if (p.tmuxSessionName) return window.api.tmuxDetach(p.sessionId).catch(() => {})
        return window.api.killPty(p.sessionId)
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
        workspaceState.selectedWorktreePath = path
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

export async function cleanupOrphanedTmuxSessions(): Promise<void> {
  const available = await window.api.tmuxIsAvailable().catch(() => false)
  if (!available) return

  // Collect tmux session names claimed by any pane in the current layout
  const claimedNames: string[] = []
  for (const tabs of Object.values(tabsByWorktree)) {
    for (const tab of tabs) {
      for (const pane of allPanes(tab.rootSplit)) {
        if (pane.tmuxSessionName) claimedNames.push(pane.tmuxSessionName)
      }
    }
  }

  // Only kill sessions that are both unclaimed by any pane AND not attached
  // by any tmux client. This avoids killing sessions the user intentionally
  // detached (e.g. closed a tab with "detach" policy). Unattached + unclaimed
  // sessions are crash orphans.
  const sessions = await window.api.tmuxListSessions().catch(() => [])
  for (const s of sessions) {
    if (!claimedNames.includes(s.name) && !s.attached) {
      await window.api.tmuxKillSession(s.name).catch(() => {})
    }
  }
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

export function getTabFocusedToolId(tab: TabInfo): string {
  const focused = findLeaf(tab.rootSplit, tab.focusedPaneId)
  return focused?.toolId ?? tab.toolId
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

export async function closeFocusedPane(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return

  const tabId = activeTabId[worktreePath]
  const tab = tabs.find((t) => t.id === tabId)
  if (!tab) return

  // Check if focused pane has active process before closing
  const focusedPane = findLeaf(tab.rootSplit, tab.focusedPaneId)
  if (focusedPane && focusedPane.isRunning) {
    const description = await getActiveProcessDescription([focusedPane])
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

  const result = treeRemovePane(tab.rootSplit, tab.focusedPaneId)
  if (!result) return

  // Kill the removed pane's PTY or destroy browser view (editor panes have no session)
  if (result.removed.paneType === 'editor') {
    // No-op
  } else if (result.removed.paneType === 'browser') {
    delete browserSessions[result.removed.sessionId]
    await window.api.teardownBrowserWebview(result.removed.sessionId)
  } else {
    await window.api.killPty(result.removed.sessionId)
  }

  if (!result.tree) {
    // Last pane — close the tab entirely
    // Remove from closed tabs push since closeTab will handle it
    // But we already killed the PTY, so we handle manually
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
  switch (zone) {
    case 'left':
      return { direction: 'vsplit', position: 'first' }
    case 'right':
      return { direction: 'vsplit', position: 'second' }
    case 'top':
      return { direction: 'hsplit', position: 'first' }
    case 'bottom':
      return { direction: 'hsplit', position: 'second' }
  }
}

export function moveTabToSplit(
  worktreePath: string,
  sourceTabId: string,
  targetTabId: string,
  targetPaneId: string,
  zone: DropZone,
): boolean {
  const tabs = tabsByWorktree[worktreePath]
  if (!tabs) return false

  const sourceTab = tabs.find((t) => t.id === sourceTabId)
  const targetTab = tabs.find((t) => t.id === targetTabId)
  if (!sourceTab || !targetTab || sourceTabId === targetTabId) return false

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

// --- Layout persistence ---

const saveTimers: Record<string, ReturnType<typeof setTimeout>> = {}

function scheduleSave(worktreePath: string): void {
  if (saveTimers[worktreePath]) clearTimeout(saveTimers[worktreePath])
  saveTimers[worktreePath] = setTimeout(() => {
    delete saveTimers[worktreePath]
    saveLayoutForWorktree(worktreePath)
  }, 500)
}

function serializeSplitNode(node: SplitNode): SerializedSplitNode {
  if (node.type === 'leaf') {
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
    if (node.pane.paneType === 'editor') {
      leaf.filePath = node.pane.filePath
    }
    if (node.pane.tmuxSessionName) {
      leaf.tmuxSessionName = node.pane.tmuxSessionName
    }
    return leaf
  }
  return {
    type: node.type,
    first: serializeSplitNode(node.first),
    second: serializeSplitNode(node.second),
    ratio: node.ratio,
  }
}

function saveLayoutForWorktree(worktreePath: string): void {
  const tabs = tabsByWorktree[worktreePath]
  const wsId = getProjectForWorktree(worktreePath)?.workspace.id ?? workspaceState.workspace?.id
  if (!tabs || !wsId) return

  const activeId = activeTabId[worktreePath]
  const activeIndex = tabs.findIndex((t) => t.id === activeId)

  const layout: SerializedLayout = {
    tabs: tabs.map((tab) => ({
      toolId: tab.toolId,
      toolName: tab.toolName,
      rootSplit: serializeSplitNode(tab.rootSplit),
    })),
    activeTabIndex: activeIndex >= 0 ? activeIndex : 0,
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

async function restoreSplitNode(
  node: SerializedSplitNode,
  worktreePath: string,
): Promise<SplitNode> {
  if (node.type === 'leaf') {
    const paneId = nextPaneId()
    let pane: PaneSession

    if (node.toolId === 'editor' && node.filePath) {
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
        filePath: node.filePath,
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
      const options: { workspaceName?: string; branch?: string; resumeSessionId?: string } = {}
      if (AI_TOOL_IDS.has(node.toolId)) {
        const project = getProjectForWorktree(worktreePath)
        options.workspaceName = project?.workspace.name ?? workspaceState.workspace?.name ?? ''
        options.branch = workspaceState.branch ?? undefined
        options.resumeSessionId = node.agentSessionId ?? node.claudeSessionId
      }
      const result = await window.api.spawnTool(node.toolId, worktreePath, options)
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

  const restoredTabs: TabInfo[] = []

  for (const serializedTab of layout.tabs) {
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
      // Skip tabs that fail to restore
    }
  }

  if (restoredTabs.length === 0) return false

  tabsByWorktree[worktreePath] = restoredTabs
  const activeIdx = Math.min(layout.activeTabIndex, restoredTabs.length - 1)
  activeTabId[worktreePath] = restoredTabs[Math.max(0, activeIdx)].id

  return true
}
