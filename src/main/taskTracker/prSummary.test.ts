import { describe, expect, it, vi } from 'vitest'
import { loadPullRequestSummary, PR_SUMMARY_FIELDS, PR_SUMMARY_TIMEOUT_MS } from './prSummary'

describe('loadPullRequestSummary', () => {
  it('requests and parses only the fields needed by the sidebar', async () => {
    const run = vi.fn().mockResolvedValue({
      stdout: JSON.stringify([{ number: 344, state: 'OPEN', isDraft: false }]),
    })

    const result = await loadPullRequestSummary('C:/repo', 'feature/large-pr', run)

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value).toEqual({ number: 344, state: 'OPEN', isDraft: false })
    expect(run).toHaveBeenCalledWith(
      'gh',
      [
        'pr',
        'list',
        '--head',
        'feature/large-pr',
        '--state',
        'open',
        '--limit',
        '1',
        '--json',
        PR_SUMMARY_FIELDS,
      ],
      {
        cwd: 'C:/repo',
        encoding: 'utf8',
        maxBuffer: 64 * 1024,
        timeout: PR_SUMMARY_TIMEOUT_MS,
        windowsHide: true,
      },
    )
    expect(PR_SUMMARY_FIELDS).toBe('number,state,isDraft')
    expect(PR_SUMMARY_TIMEOUT_MS).toBe(15_000)
  })

  it('returns null only when the branch has no pull request', async () => {
    const run = vi.fn().mockResolvedValue({ stdout: '[]' })

    const result = await loadPullRequestSummary('C:/repo', 'feature/without-pr', run)

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value).toBeNull()
    expect(run).toHaveBeenCalledTimes(2)
  })

  it('falls back to the latest closed pull request when no open one exists', async () => {
    const run = vi
      .fn()
      .mockResolvedValueOnce({ stdout: '[]' })
      .mockResolvedValueOnce({
        stdout: JSON.stringify([{ number: 343, state: 'MERGED', isDraft: false }]),
      })

    const result = await loadPullRequestSummary('C:/repo', 'feature/merged-pr', run)

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value).toEqual({ number: 343, state: 'MERGED', isDraft: false })
    expect(run.mock.calls.map((call) => call[1])).toEqual([
      expect.arrayContaining(['--state', 'open']),
      expect.arrayContaining(['--state', 'closed']),
    ])
  })

  it.each([
    ['invalid JSON', { stdout: '{' }],
    ['wrong shape', { stdout: JSON.stringify([{ number: '344', state: 'OPEN' }]) }],
  ])('returns an error for %s', async (_label, commandResult) => {
    const run = vi.fn().mockResolvedValue(commandResult)

    const result = await loadPullRequestSummary('C:/repo', 'feature/large-pr', run)

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected PR lookup to fail')
    expect(result.error._tag).toBe('PRLookupFailed')
  })

  it('returns an error when GitHub CLI fails or times out', async () => {
    const run = vi.fn().mockRejectedValue(new Error('timed out'))

    const result = await loadPullRequestSummary('C:/repo', 'feature/large-pr', run)

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected PR lookup to fail')
    expect(result.error).toEqual({ _tag: 'PRLookupFailed', reason: 'timed out' })
  })

  it('treats a missing GitHub CLI as an unavailable optional fallback', async () => {
    const missingGh = Object.assign(new Error('spawn gh ENOENT'), { code: 'ENOENT' })
    const run = vi.fn().mockRejectedValue(missingGh)

    const result = await loadPullRequestSummary('C:/repo', 'feature/large-pr', run)

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value).toBeNull()
  })
})
