import { describe, expect, it } from 'vitest'
import { redactGitHubFailureReason } from './redactFailureReason'

describe('redactGitHubFailureReason', () => {
  it('removes credentials embedded in remotes and standalone GitHub tokens', () => {
    const token = `ghp_${'a'.repeat(36)}`
    const reason = `failed for https://x-access-token:${token}@github.com/owner/repo (${token})`

    const redacted = redactGitHubFailureReason(reason)

    expect(redacted).toBe('failed for https://[redacted]@github.com/owner/repo ([redacted])')
    expect(redacted).not.toContain(token)
  })
})
