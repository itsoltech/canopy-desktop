import { err, ok, type Result } from 'neverthrow'
import { describe, expect, it } from 'vitest'
import { repoConfigOrNull, type TaskTrackerError } from './errors'
import type { RepoConfig } from './types'

const config: RepoConfig = {
  version: 1,
  trackers: [],
  projectOverrides: {},
  filters: { assignedToMe: true, statuses: [] },
}

describe('repoConfigOrNull', () => {
  it('returns a loaded repository config', () => {
    const result: Result<RepoConfig, TaskTrackerError> = ok(config)

    expect(repoConfigOrNull(result)).toBe(config)
  })

  it('returns null only for a missing repository config', () => {
    const result: Result<RepoConfig, TaskTrackerError> = err({
      _tag: 'ConfigNotFound',
      repoRoot: '/repo',
    })

    expect(repoConfigOrNull(result)).toBeNull()
  })

  it('propagates parse and read failures instead of allowing a global fallback', () => {
    const failures: Array<
      Extract<TaskTrackerError, { _tag: 'ConfigParseError' | 'ConfigReadError' }>
    > = [
      { _tag: 'ConfigParseError', repoRoot: '/repo', reason: 'too large' },
      { _tag: 'ConfigReadError', repoRoot: '/repo', reason: 'permission denied' },
    ]

    for (const failure of failures) {
      const result: Result<RepoConfig, TaskTrackerError> = err(failure)
      expect(() => repoConfigOrNull(result)).toThrow(failure.reason)
    }
  })
})
