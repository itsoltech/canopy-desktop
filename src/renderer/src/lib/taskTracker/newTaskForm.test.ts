import { describe, it, expect } from 'vitest'
import {
  visibleFields,
  filterBoardsForProject,
  buildAssigneeOptions,
  buildSprintOptions,
  validateTitle,
} from './newTaskForm'

describe('visibleFields', () => {
  it('shows project, type, board and sprint for jira', () => {
    expect(visibleFields('jira')).toEqual({
      project: true,
      type: true,
      board: true,
      sprint: true,
      sprintLabel: 'Sprint',
    })
  })

  it('shows project, type, board and sprint for youtrack', () => {
    expect(visibleFields('youtrack')).toEqual({
      project: true,
      type: true,
      board: true,
      sprint: true,
      sprintLabel: 'Sprint',
    })
  })

  it('hides project/type/board for github and labels sprints as milestones', () => {
    expect(visibleFields('github')).toEqual({
      project: false,
      type: false,
      board: false,
      sprint: true,
      sprintLabel: 'Milestone',
    })
  })
})

describe('filterBoardsForProject', () => {
  const boards = [
    { id: '1', name: 'Alpha board', projectKey: 'ALPHA' },
    { id: '2', name: 'Beta board', projectKey: 'BETA' },
    { id: '3', name: 'Shared board' },
  ]

  it('keeps boards of the project plus boards without a project', () => {
    expect(filterBoardsForProject(boards, 'ALPHA').map((b) => b.id)).toEqual(['1', '3'])
  })

  it('matches the project key case-insensitively', () => {
    expect(filterBoardsForProject(boards, 'alpha').map((b) => b.id)).toEqual(['1', '3'])
  })

  it('returns all boards when no project is selected', () => {
    expect(filterBoardsForProject(boards, '').map((b) => b.id)).toEqual(['1', '2', '3'])
  })
})

describe('buildAssigneeOptions', () => {
  it('prepends an Unassigned empty option', () => {
    const options = buildAssigneeOptions([
      { id: 'u1', displayName: 'Ada' },
      { id: 'u2', displayName: 'Grace' },
    ])
    expect(options[0]).toEqual({ value: '', label: 'Unassigned' })
    expect(options.slice(1)).toEqual([
      { value: 'u1', label: 'Ada' },
      { value: 'u2', label: 'Grace' },
    ])
  })
})

describe('buildSprintOptions', () => {
  it('prepends a none option with the provided label', () => {
    const options = buildSprintOptions(
      [
        { id: 's1', name: 'Sprint 1', state: 'active' as const },
        { id: 's2', name: 'Sprint 2', state: 'future' as const },
      ],
      'Backlog (none)',
    )
    expect(options[0]).toEqual({ value: '', label: 'Backlog (none)' })
    expect(options.slice(1)).toEqual([
      { value: 's1', label: 'Sprint 1' },
      { value: 's2', label: 'Sprint 2' },
    ])
  })
})

describe('validateTitle', () => {
  it('rejects empty and whitespace-only titles', () => {
    expect(validateTitle('')).not.toBeNull()
    expect(validateTitle('   ')).not.toBeNull()
  })

  it('rejects titles longer than 512 characters', () => {
    expect(validateTitle('x'.repeat(513))).not.toBeNull()
  })

  it('accepts a normal title', () => {
    expect(validateTitle('Fix the login flow')).toBeNull()
  })
})
