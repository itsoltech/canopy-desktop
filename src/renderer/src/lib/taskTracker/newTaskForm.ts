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
}

export function buildAssigneeOptions(
  users: Array<{ id: string; displayName: string }>,
): SelectOption[] {
  return [
    { value: '', label: 'Unassigned' },
    ...users.map((u) => ({ value: u.id, label: u.displayName })),
  ]
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
