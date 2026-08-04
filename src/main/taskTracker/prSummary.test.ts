import { describe, expect, it, vi } from 'vitest'
import { loadPullRequestSummary, PR_SUMMARY_FIELDS, PR_SUMMARY_TIMEOUT_MS } from './prSummary'

describe('loadPullRequestSummary', () => {
  it('requests and parses only the fields needed by the sidebar', async () => {
    const run = vi.fn().mockResolvedValue({
      stdout: JSON.stringify({ number: 344, state: 'OPEN', isDraft: false }),
    })

    const summary = await loadPullRequestSummary('C:/repo', 'feature/large-pr', run)

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

  it.each([
    ['invalid JSON', { stdout: '{' }],
    ['wrong shape', { stdout: JSON.stringify({ number: '344', state: 'OPEN' }) }],
  ])('returns null for %s', async (_label, result) => {
    const run = vi.fn().mockResolvedValue(result)

    await expect(loadPullRequestSummary('C:/repo', 'feature/large-pr', run)).resolves.toBeNull()
  })

  it('returns null when GitHub CLI fails or times out', async () => {
    const run = vi.fn().mockRejectedValue(new Error('timed out'))

    await expect(loadPullRequestSummary('C:/repo', 'feature/large-pr', run)).resolves.toBeNull()
  })
})
