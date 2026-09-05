import { describe, expect, it } from 'vitest'
import {
  gitHubCliFailureReason,
  isMissingGitHubCli,
  redactGitHubFailureReason,
} from './redactFailureReason'
import { errorMessage } from '../errors'

describe('redactGitHubFailureReason', () => {
  it('removes credentials embedded in remotes and standalone GitHub tokens', () => {
    const token = `ghp_${'a'.repeat(36)}`
    const reason = `failed for https://x-access-token:${token}@github.com/owner/repo (${token})`

    const redacted = redactGitHubFailureReason(reason)

    expect(redacted).toBe('failed for https://[redacted]@github.com/owner/repo ([redacted])')
    expect(redacted).not.toContain(token)
  })

  it("redacts an execFile error's own message, which is what git:createPR re-raises", () => {
    // `gh pr create` fails with a Node execFile error whose MESSAGE already
    // carries the command and stderr — the handler wraps errorMessage(err), so
    // the push remote printed on failure must not survive that path.
    const token = `ghp_${'c'.repeat(36)}`
    const execFileError = Object.assign(
      new Error(
        `Command failed: gh pr create --title x\nfatal: could not read from https://x-access-token:${token}@github.com/owner/repo`,
      ),
      { code: 1, stderr: `fatal: https://x-access-token:${token}@github.com/owner/repo` },
    )

    const redacted = redactGitHubFailureReason(errorMessage(execFileError))

    expect(redacted).toContain('https://[redacted]@github.com/owner/repo')
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
