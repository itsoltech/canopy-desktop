import { describe, expect, it, vi } from 'vitest'
import { loadPullRequestSummary, PR_SUMMARY_FIELDS, PR_SUMMARY_TIMEOUT_MS } from './prSummary'

describe('loadPullRequestSummary', () => {
  it('requests and parses only the fields needed by the sidebar', async () => {
    const run = vi.fn().mockResolvedValue({
      stdout: JSON.stringify({ number: 344, state: 'OPEN', isDraft: false }),
    })

    const summary = await loadPullRequestSummary('C:/repo', 'feature/large-pr', 0, run)

    expect(summary).toEqual({ number: 344, state: 'OPEN', isDraft: false })
    expect(run).toHaveBeenCalledWith(
      'gh',
      ['pr', 'view', 'feature/large-pr', '--json', PR_SUMMARY_FIELDS],
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

  it('coalesces identical in-flight commands in the main process', async () => {
    const result = Promise.withResolvers<{ stdout: string }>()
    const run = vi.fn().mockReturnValue(result.promise)

    const first = loadPullRequestSummary('C:\\repo', 'feature/large-pr', 0, run)
    const duplicate = loadPullRequestSummary('C:/repo', 'feature/large-pr', 0, run)

    expect(duplicate).toBe(first)
    expect(run).toHaveBeenCalledOnce()

    result.resolve({
      stdout: JSON.stringify({ number: 344, state: 'OPEN', isDraft: false }),
    })
    await expect(first).resolves.toMatchObject({ number: 344 })
  })

  it('runs a newer generation after the previous command exits', async () => {
    const firstResult = Promise.withResolvers<{ stdout: string }>()
    const run = vi
      .fn()
      .mockReturnValueOnce(firstResult.promise)
      .mockResolvedValueOnce({
        stdout: JSON.stringify({ number: 344, state: 'MERGED', isDraft: false }),
      })

    const stale = loadPullRequestSummary('C:/repo', 'feature/large-pr', 0, run)
    const refreshed = loadPullRequestSummary('C:/repo', 'feature/large-pr', 1, run)

    expect(run).toHaveBeenCalledOnce()
    firstResult.resolve({
      stdout: JSON.stringify({ number: 344, state: 'OPEN', isDraft: false }),
    })

    await expect(stale).resolves.toMatchObject({ state: 'OPEN' })
    await expect(refreshed).resolves.toMatchObject({ state: 'MERGED' })
    expect(run).toHaveBeenCalledTimes(2)
  })

  it.each([
    ['invalid JSON', { stdout: '{' }],
    ['wrong shape', { stdout: JSON.stringify({ number: '344', state: 'OPEN' }) }],
  ])('returns null for %s', async (_label, result) => {
    const run = vi.fn().mockResolvedValue(result)

    await expect(loadPullRequestSummary('C:/repo', 'feature/large-pr', 0, run)).resolves.toBeNull()
  })

  it('returns null when GitHub CLI fails or times out', async () => {
    const run = vi.fn().mockRejectedValue(new Error('timed out'))

    await expect(loadPullRequestSummary('C:/repo', 'feature/large-pr', 0, run)).resolves.toBeNull()
  })
})
