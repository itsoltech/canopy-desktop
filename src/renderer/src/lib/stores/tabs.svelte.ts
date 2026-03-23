export interface TabInfo {
  id: string
  toolId: string
  toolName: string
  name: string
  sessionId: string
  wsUrl: string
  worktreePath: string
  isRunning: boolean
  exitCode: number | null
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

export async function openTool(toolId: string, worktreePath: string): Promise<TabInfo> {
  const result = await window.api.spawnTool(toolId, worktreePath)
  const id = nextTabId()
  const name = computeDisplayName(result.toolName, worktreePath, toolId)

  const tab: TabInfo = {
    id,
    toolId,
    toolName: result.toolName,
    name,
    sessionId: result.sessionId,
    wsUrl: result.wsUrl,
    worktreePath,
    isRunning: true,
    exitCode: null
  }

  if (!tabsByWorktree[worktreePath]) {
    tabsByWorktree[worktreePath] = []
  }
  tabsByWorktree[worktreePath].push(tab)
  activeTabId[worktreePath] = id

  return tab
}

export async function closeTab(tabId: string): Promise<void> {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    const idx = tabs.findIndex((t) => t.id === tabId)
    if (idx === -1) continue

    const tab = tabs[idx]

    // Push to closed tabs stack
    if (!closedTabs[path]) closedTabs[path] = []
    closedTabs[path].push({
      toolId: tab.toolId,
      toolName: tab.toolName,
      worktreePath: path,
      closedAt: Date.now()
    })
    if (closedTabs[path].length > MAX_CLOSED_TABS) {
      closedTabs[path].shift()
    }

    // Kill PTY
    await window.api.killPty(tab.sessionId)

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
        // Auto-open a shell tab when last tab is closed
        await openTool('shell', path)
      }
    }

    return
  }
}

export function switchTab(tabId: string): void {
  for (const [path, tabs] of Object.entries(tabsByWorktree)) {
    if (tabs.some((t) => t.id === tabId)) {
      activeTabId[path] = tabId
      return
    }
  }
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
  return tabs.filter((t) => t.toolId === toolId && t.isRunning).length
}

export function handlePtyExit(sessionId: string, exitCode: number): void {
  for (const tabs of Object.values(tabsByWorktree)) {
    const tab = tabs.find((t) => t.sessionId === sessionId)
    if (tab) {
      tab.isRunning = false
      tab.exitCode = exitCode
      return
    }
  }
}

export async function restartTab(tabId: string): Promise<void> {
  for (const tabs of Object.values(tabsByWorktree)) {
    const idx = tabs.findIndex((t) => t.id === tabId)
    if (idx === -1) continue

    const tab = tabs[idx]

    // Kill old PTY
    await window.api.killPty(tab.sessionId)

    // Spawn new
    const result = await window.api.spawnTool(tab.toolId, tab.worktreePath)

    // Replace with a new tab ID so Svelte remounts the TerminalInstance
    const newId = nextTabId()
    tabs[idx] = {
      ...tab,
      id: newId,
      sessionId: result.sessionId,
      wsUrl: result.wsUrl,
      isRunning: true,
      exitCode: null
    }

    if (activeTabId[tab.worktreePath] === tabId) {
      activeTabId[tab.worktreePath] = newId
    }

    return
  }
}

export async function ensureShellTab(worktreePath: string): Promise<void> {
  const tabs = tabsByWorktree[worktreePath]
  if (tabs && tabs.length > 0) return
  await openTool('shell', worktreePath)
}

export async function killAllTabs(): Promise<void> {
  const allTabs = Object.values(tabsByWorktree).flat()
  await Promise.all(allTabs.map((t) => window.api.killPty(t.sessionId)))
  for (const path of Object.keys(tabsByWorktree)) {
    delete tabsByWorktree[path]
    delete activeTabId[path]
  }
}

export function getAllTabs(): TabInfo[] {
  return Object.values(tabsByWorktree).flat()
}
