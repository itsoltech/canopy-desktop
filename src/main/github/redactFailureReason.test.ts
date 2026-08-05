import { describe, expect, it } from 'vitest'
import {
  gitHubCliFailureReason,
  isMissingGitHubCli,
  redactGitHubFailureReason,
} from './redactFailureReason'

describe('redactGitHubFailureReason', () => {
  it('removes credentials embedded in remotes and standalone GitHub tokens', () => {
    const token = `ghp_${'a'.repeat(36)}`
    const reason = `failed for https://x-access-token:${token}@github.com/owner/repo (${token})`

    const redacted = redactGitHubFailureReason(reason)

    expect(redacted).toBe('failed for https://[redacted]@github.com/owner/repo ([redacted])')
    expect(redacted).not.toContain(token)
  })

  it('classifies timeout and stderr failures through the shared redacting path', () => {
    const token = `ghp_${'b'.repeat(36)}`

    expect(gitHubCliFailureReason({ killed: true }, 15_000)).toBe(
      'GitHub CLI request timed out after 15 seconds',
    )
    expect(gitHubCliFailureReason(new Error('request failed'), 30_000, `bad ${token}`)).toBe(
      'bad [redacted]',
    )
    expect(isMissingGitHubCli({ code: 'ENOENT' })).toBe(true)
  })
})
