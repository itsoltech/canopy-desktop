import { describe, expect, it } from 'vitest'
import { prCliFailure, PR_COMMAND_TIMEOUT_MS } from './prCreation'
import { isSafeGitRefName } from '../../renderer-shared/gitRef'

describe('isSafeGitRefName', () => {
  it.each(['feature/ISSUE-123-description', 'release/1.2.3', 'dependabot/npm_and_yarn/pkg-2.0.0'])(
    'accepts safe branch ref %s',
    (branch) => {
      expect(isSafeGitRefName(branch)).toBe(true)
    },
  )

  it.each([
    '',
    ' feature/leading-space',
    'feature/trailing-space ',
    '-feature/flag',
    'feature//double',
    'feature/../escape',
    'feature/query?value',
  ])('rejects unsafe branch ref %j', (branch) => {
    expect(isSafeGitRefName(branch)).toBe(false)
  })
})

describe('prCliFailure', () => {
  it.each(['create', 'merge', 'close', 'branch lookup', 'remote branch delete'])(
    'redacts credential-bearing stderr for the %s command path',
    () => {
      const token = `ghp_${'a'.repeat(36)}`
      const error = Object.assign(new Error('gh failed'), {
        stderr: `failed for https://x-access-token:${token}@github.com/owner/repo`,
      })

      expect(prCliFailure(error)).toEqual({
        _tag: 'PRCreationFailed',
        reason: 'failed for https://[redacted]@github.com/owner/repo',
      })
    },
  )

  it('reports the configured command timeout without exposing process output', () => {
    expect(prCliFailure({ killed: true, stderr: 'secret output' })).toEqual({
      _tag: 'PRCreationFailed',
      reason: `GitHub CLI request timed out after ${PR_COMMAND_TIMEOUT_MS / 1000} seconds`,
    })
  })
})
