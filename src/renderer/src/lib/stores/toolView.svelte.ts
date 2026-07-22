import { getPref, setPref } from './preferences.svelte'
import { getTools } from './tools.svelte'

export interface ToolViewEntry {
  id: string
  visible: boolean
}

const PREF_KEY = 'tools.view'

function parseSaved(raw: string): ToolViewEntry[] {
  if (!raw) return []
  try {
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter(
      (e): e is ToolViewEntry =>
        typeof e === 'object' &&
        e !== null &&
        typeof e.id === 'string' &&
        typeof e.visible === 'boolean',
    )
  } catch {
    return []
  }
}

/**
 * Reconcile the saved sidebar view config against the current tool set.
 * Known ids keep their saved order and visibility; newly added tools are
 * appended as visible; removed tools drop out. Reads reactive prefs and tool
 * state, so callers can use it inside `$derived`/templates.
 */
export function getToolView(): ToolViewEntry[] {
  const saved = parseSaved(getPref(PREF_KEY))
  const tools = getTools()
  const knownIds = tools.map((t) => t.id)
  const result: ToolViewEntry[] = []
  const has = (id: string): boolean => result.some((r) => r.id === id)
  for (const entry of saved) {
    if (knownIds.includes(entry.id) && !has(entry.id)) {
      result.push({ id: entry.id, visible: entry.visible })
    }
  }
  for (const tool of tools) {
    if (!has(tool.id)) {
      result.push({ id: tool.id, visible: true })
    }
  }
  return result
}

function persist(view: ToolViewEntry[]): void {
  void setPref(PREF_KEY, JSON.stringify(view))
}

export function toggleToolVisibility(id: string): void {
  const next = getToolView().map((e) => (e.id === id ? { ...e, visible: !e.visible } : e))
  persist(next)
}

export function moveToolUp(id: string): void {
  const view = getToolView()
  const i = view.findIndex((e) => e.id === id)
  if (i <= 0) return
  const next = [...view]
  ;[next[i - 1], next[i]] = [next[i], next[i - 1]]
  persist(next)
}

export function moveToolDown(id: string): void {
  const view = getToolView()
  const i = view.findIndex((e) => e.id === id)
  if (i === -1 || i === view.length - 1) return
  const next = [...view]
  ;[next[i], next[i + 1]] = [next[i + 1], next[i]]
  persist(next)
}
