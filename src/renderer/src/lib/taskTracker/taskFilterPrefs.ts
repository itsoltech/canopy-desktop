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
export const NO_SPRINT = '(backlog)'
/** The bucket's previous label — mapped on read so saved filters keep working. */
const LEGACY_NO_SPRINT = '(no sprint)'

/** Sort order for status chips: workflow direction (to-do → in progress → done), then the
 *  tracker's own configured order within a category, then alphabetically. */
const CATEGORY_RANK: Record<string, number> = { todo: 0, 'in-progress': 1, done: 2 }

export interface StatusMeta {
  category?: string
  order: number
}

/** name → {category, order} from the tracker's configured status list. */
export function buildStatusMeta(
  statuses: Array<{ name: string; statusCategory?: string }>,
): Map<string, StatusMeta> {
  const meta = new Map<string, StatusMeta>()
  statuses.forEach((s, i) => {
    if (!meta.has(s.name)) meta.set(s.name, { category: s.statusCategory, order: i })
  })
  return meta
}

export function sortStatuses(names: string[], meta: Map<string, StatusMeta>): string[] {
  return [...names].sort((a, b) => {
    const ma = meta.get(a)
    const mb = meta.get(b)
    const ra = CATEGORY_RANK[ma?.category ?? ''] ?? 99
    const rb = CATEGORY_RANK[mb?.category ?? ''] ?? 99
    if (ra !== rb) return ra - rb
    const oa = ma?.order ?? Number.MAX_SAFE_INTEGER
    const ob = mb?.order ?? Number.MAX_SAFE_INTEGER
    if (oa !== ob) return oa - ob
    return a.localeCompare(b)
  })
}

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
    const parsed = JSON.parse(raw) as SavedTaskFilters
    parsed.excludedSprints = parsed.excludedSprints?.map((s) =>
      s === LEGACY_NO_SPRINT ? NO_SPRINT : s,
    )
    return parsed
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
