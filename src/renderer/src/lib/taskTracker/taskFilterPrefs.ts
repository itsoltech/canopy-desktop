import { getPref, setPref } from '../stores/preferences.svelte'

/**
 * Task-list filter persistence shared by the task picker and the Create Worktree "From task"
 * screen — one pref per connection + board, so selections made in either place apply to both.
 */
export interface SavedTaskFilters {
  excludedStatuses: string[]
  excludedSprints?: string[]
  assignedToMe: boolean
}

/** Statuses treated as "done" and excluded by default on the first visit to a board. */
export const DONE_STATUS_PATTERN =
  /^(done|closed|resolved|cancelled|rejected|complete|gotowe|zamkni)/i

/** Bucket for tasks without a sprint so they can be filtered like any other sprint value. */
export const NO_SPRINT = '(no sprint)'

function prefKey(connectionId: string, boardId: string): string {
  return `taskTracker.pickerFilters.${connectionId}.${boardId}`
}

export function loadSavedTaskFilters(
  connectionId: string,
  boardId: string,
): SavedTaskFilters | null {
  const raw = getPref(prefKey(connectionId, boardId))
  if (!raw) return null
  try {
    return JSON.parse(raw) as SavedTaskFilters
  } catch {
    return null
  }
}

export function saveTaskFilters(
  connectionId: string,
  boardId: string,
  filters: SavedTaskFilters,
): void {
  if (!boardId) return
  setPref(prefKey(connectionId, boardId), JSON.stringify(filters))
}

/** Display key for a task row: subtasks render as PARENT/SUB (e.g. GAKKO-123/GAKKO-345). */
export function taskDisplayKey(task: { key: string; parentKey?: string }): string {
  return task.parentKey ? `${task.parentKey}/${task.key}` : task.key
}
