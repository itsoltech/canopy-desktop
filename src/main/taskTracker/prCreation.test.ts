import { afterEach, describe, expect, it, vi } from 'vitest'
import { okAsync } from 'neverthrow'
import {
  closePullRequest,
  createPullRequest,
  deleteRemoteBranch,
  mergePullRequest,
  prCliFailure,
  PR_COMMAND_TIMEOUT_MS,
  remoteBranchExists,
  type PRCommandRunner,
} from './prCreation'
import { isSafeGitRefName } from '../../renderer-shared/gitRef'
import { GitRepository } from '../git/GitRepository'

afterEach(() => {
  vi.restoreAllMocks()
})

const token = `ghp_${'a'.repeat(36)}`

function credentialBearingError(): Error {
  return Object.assign(new Error('gh failed'), {
    stderr: `failed for https://x-access-token:${token}@github.com/owner/repo`,
  })
}

const rejectingRunner: PRCommandRunner = () => Promise.reject(credentialBearingError())

function expectRedactedFailure(result: { _unsafeUnwrapErr(): unknown }): void {
  expect(result._unsafeUnwrapErr()).toEqual({
    _tag: 'PRCreationFailed',
    reason: 'failed for https://[redacted]@github.com/owner/repo',
  })
}

describe('isSafeGitRefName', () => {
  it.each([
    'feature/ISSUE-123-description',
    'release/1.2.3',
    'dependabot/npm_and_yarn/pkg-2.0.0',
    'release#1',
    'percent%done',
  ])('accepts safe branch ref %s', (branch) => {
    expect(isSafeGitRefName(branch)).toBe(true)
  })

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
  it('redacts credential-bearing stderr', () => {
    expect(prCliFailure(credentialBearingError())).toEqual({
      _tag: 'PRCreationFailed',
      reason: 'failed for https://[redacted]@github.com/owner/repo',
    })
  })

  it('reports the configured command timeout without exposing process output', () => {
    expect(prCliFailure({ killed: true, stderr: 'secret output' })).toEqual({
      _tag: 'PRCreationFailed',
      reason: `GitHub CLI request timed out after ${PR_COMMAND_TIMEOUT_MS / 1000} seconds`,
    })
  })
})

describe('PR command failure paths', () => {
  it('redacts a failure from the actual create command', async () => {
    vi.spyOn(GitRepository, 'push').mockReturnValue(okAsync({ branch: 'feature/test', remote: '' }))
    const runner: PRCommandRunner = (_repoRoot, args) => {
      if (args[0] === '--version') return Promise.resolve({ stdout: 'gh version', stderr: '' })
      if (args[0] === 'pr' && args[1] === 'list') {
        return Promise.resolve({ stdout: '', stderr: '' })
      }
      return Promise.reject(credentialBearingError())
    }

    const result = await createPullRequest(
      {
        repoRoot: 'C:\\source\\repo',
        sourceBranch: 'feature/test',
        task: {
          key: 'TEST-1',
          summary: 'Test PR failure',
          description: '',
          status: 'In progress',
          priority: 'Normal',
          type: 'task',
        },
        prConfig: {
          titleTemplate: '{taskKey}: {taskTitle}',
          bodyTemplate: '{taskDescription}',
          defaultTargetBranch: 'develop',
          targetRules: [],
        },
      },
      runner,
    )

    expectRedactedFailure(result)
  })

  it('redacts failures from merge and close commands', async () => {
    expectRedactedFailure(
      await mergePullRequest('C:\\source\\repo', 42, 'squash', false, rejectingRunner),
    )
    expectRedactedFailure(await closePullRequest('C:\\source\\repo', 42, false, rejectingRunner))
  })

  it('redacts a failure from remote branch deletion', async () => {
    expectRedactedFailure(
      await deleteRemoteBranch('C:\\source\\repo', 'feature/test', rejectingRunner),
    )
  })

  it('routes branch lookup failures through the redacting mapper before applying fallback', async () => {
    let stderrRead = false
    const error = new Error('gh failed')
    Object.defineProperty(error, 'stderr', {
      get: () => {
        stderrRead = true
        return `failed for https://x-access-token:${token}@github.com/owner/repo`
      },
    })
    const runner: PRCommandRunner = () => Promise.reject(error)

    const result = await remoteBranchExists('C:\\source\\repo', 'feature/test', runner)

    expect(result._unsafeUnwrap()).toBe(true)
    expect(stderrRead).toBe(true)
  })

  it('URL-encodes valid ref metacharacters in GitHub API paths', async () => {
    const calls: string[][] = []
    const runner: PRCommandRunner = (_repoRoot, args) => {
      calls.push(args)
      return Promise.resolve({ stdout: '', stderr: '' })
    }

    expect(
      (await remoteBranchExists('C:\\source\\repo', 'feature/release#100%', runner)).isOk(),
    ).toBe(true)
    expect(
      (await deleteRemoteBranch('C:\\source\\repo', 'feature/release#100%', runner)).isOk(),
    ).toBe(true)
    expect(calls[0]).toContain('repos/{owner}/{repo}/branches/feature%2Frelease%23100%25')
    expect(calls[1]).toContain('repos/{owner}/{repo}/git/refs/heads/feature%2Frelease%23100%25')
  })
})
