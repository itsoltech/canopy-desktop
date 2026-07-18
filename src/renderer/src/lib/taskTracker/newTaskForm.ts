import { match } from 'ts-pattern'
import type { TrackerProviderKind } from './types'

export interface NewTaskFieldVisibility {
  project: boolean
  type: boolean
  board: boolean
  sprint: boolean
  sprintLabel: string
}

/** Which create-form fields a provider supports (GitHub issues have no project/type/board;
 *  its milestones stand in for sprints). */
export function visibleFields(provider: TrackerProviderKind): NewTaskFieldVisibility {
  return match(provider)
    .with('github', () => ({
      project: false,
      type: false,
      board: false,
      sprint: true,
      sprintLabel: 'Milestone',
    }))
    .otherwise(() => ({
      project: true,
      type: true,
      board: true,
      sprint: true,
      sprintLabel: 'Sprint',
    }))
}

export interface BoardLike {
  id: string
  name: string
  projectKey?: string
}

/** Boards that can host the new task's sprint: the selected project's own boards plus boards
 *  not tied to any project. No selected project = no filtering. */
export function filterBoardsForProject<T extends BoardLike>(boards: T[], projectKey: string): T[] {
  if (!projectKey) return boards
  const wanted = projectKey.toUpperCase()
  return boards.filter((b) => !b.projectKey || b.projectKey.toUpperCase() === wanted)
}

export interface SelectOption {
  value: string
  label: string
  icon?: string
  iconClass?: string
}

/** `icons` maps a remote icon/avatar URL to the CSP-safe data: URL resolved via the image proxy —
 *  options only carry an icon when its URL actually resolved. */
export function buildAssigneeOptions(
  users: Array<{ id: string; displayName: string; avatarUrl?: string }>,
  icons: Record<string, string> = {},
): SelectOption[] {
  return [
    { value: '', label: 'Unassigned' },
    ...users.map((u) => ({
      value: u.id,
      label: u.displayName,
      ...(u.avatarUrl && icons[u.avatarUrl]
        ? { icon: icons[u.avatarUrl], iconClass: 'rounded-full' }
        : {}),
    })),
  ]
}

export function buildTypeOptions(
  types: Array<{ name: string; iconUrl?: string }>,
  icons: Record<string, string> = {},
): SelectOption[] {
  return types.map((t) => ({
    value: t.name,
    label: t.name,
    ...(t.iconUrl && icons[t.iconUrl] ? { icon: icons[t.iconUrl], iconClass: 'rounded-sm' } : {}),
  }))
}

export function buildSprintOptions(
  sprints: Array<{ id: string; name: string }>,
  noneLabel: string,
): SelectOption[] {
  return [{ value: '', label: noneLabel }, ...sprints.map((s) => ({ value: s.id, label: s.name }))]
}

/** Returns a user-facing error, or null when the title is submittable. Mirrors the IPC limit. */
export function validateTitle(title: string): string | null {
  const trimmed = title.trim()
  if (!trimmed) return 'A title is required'
  if (trimmed.length > 512) return 'Title is too long (max 512 characters)'
  return null
}

/** Renderer-side twin of the main process's slugify — used only for the pre-create draft. */
export function slugifyTitle(text: string, maxLength = 50): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, maxLength)
}

interface BranchTemplateSource {
  branchTemplate?: { template?: string }
  projectOverrides?: Record<string, { branchTemplate?: { template?: string } } | undefined>
}

/** Effective branch template for a project — override first, then root, then the built-in. */
export function branchTemplateFor(
  config: BranchTemplateSource | undefined,
  projectKey: string,
): string {
  const override = config?.projectOverrides?.[projectKey.toUpperCase()]?.branchTemplate?.template
  return override || config?.branchTemplate?.template || '{branchType}/{taskKey}-{taskTitle}'
}

/**
 * Render a branch-name DRAFT before the task exists: known variables are substituted (an empty
 * value drops its preceding separator, like the real renderer), while unknown placeholders —
 * {taskKey} above all — stay literal and are replaced after the tracker assigns the key.
 */
export function renderBranchDraft(template: string, vars: Record<string, string>): string {
  let result = template.replace(/\{\?\w+\}/g, '').replace(/\{\/\w+\}/g, '')
  result = result.replace(/([/_-]?)\{(\w+)\}/g, (match, sep: string, key: string) => {
    if (!(key in vars)) return match
    const value = vars[key]
    return value ? `${sep}${value}` : ''
  })
  return result.replace(/\/{2,}/g, '/').replace(/^[/_-]+|[/_-]+$/g, '')
}

/** Split template text into segments for the highlight-overlay editors — `{key}` tokens found in
 *  `known` render as highlighted fields, everything else as plain text. */
export function segmentsOf(
  text: string,
  known: Set<string>,
): Array<{ text: string; field: boolean }> {
  const segs: Array<{ text: string; field: boolean }> = []
  let last = 0
  for (const m of text.matchAll(/\{(\w+)\}/g)) {
    if (m.index > last) segs.push({ text: text.slice(last, m.index), field: false })
    segs.push({ text: m[0], field: known.has(m[1]) })
    last = m.index + m[0].length
  }
  if (last < text.length) segs.push({ text: text.slice(last), field: false })
  return segs
}
