import { describe, expect, it } from 'vitest'
import { resolveTargetBranch } from './prTemplate'
import type { PRTargetRule, TrackerTask } from './types'

const subtask = (parentKey?: string): TrackerTask => ({
  key: 'ABC-11',
  summary: 'Validate email',
  description: '',
  status: 'To Do',
  priority: 'Medium',
  type: 'subtask',
  parentKey,
})

const rules: PRTargetRule[] = [{ taskType: 'subtask', targetPattern: 'feature/{parentKey}' }]

describe('resolveTargetBranch', () => {
  it('does not match branches whose key merely shares a prefix', () => {
    const branches = ['feature/ABC-1', 'feature/ABC-100']

    expect(resolveTargetBranch(subtask('ABC-10'), 'develop', rules, branches)).toBe(
      'feature/ABC-10',
    )
  })

  it('prefers an exact branch over one that extends the pattern', () => {
    const branches = ['feature/ABC-10-login-page', 'feature/ABC-10']

    expect(resolveTargetBranch(subtask('ABC-10'), 'develop', rules, branches)).toBe(
      'feature/ABC-10',
    )
  })

  it('matches a branch that contains the pattern as whole path segments', () => {
    expect(
      resolveTargetBranch(subtask('ABC-10'), 'develop', rules, ['feature/ABC-10-login-page']),
    ).toBe('feature/ABC-10-login-page')
    expect(
      resolveTargetBranch(subtask('ABC-10'), 'develop', rules, ['alice/feature/ABC-10-login']),
    ).toBe('alice/feature/ABC-10-login')
  })

  it('falls back to the default branch when the task has no parent to fill the pattern', () => {
    expect(resolveTargetBranch(subtask(), 'develop', rules, ['feature/ABC-7-other'])).toBe(
      'develop',
    )
  })
})
