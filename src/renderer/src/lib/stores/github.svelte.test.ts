import { beforeEach, describe, expect, it, vi } from 'vitest'

const api = {
  githubFetchBranchPRs: vi.fn(),
  taskTrackerPRSummary: vi.fn(),
}

vi.stubGlobal('window', { api })

import {
  PR_FALLBACK_TTL_MS,
  getPRFallbackGeneration,
  invalidatePRFallback,
  loadBranchPRs,
  loadPRFallbackSummary,
  resetGitHubState,
} from './github.svelte'

describe('GitHub PR fallback refresh', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    vi.clearAllMocks()
    resetGitHubState()
    api.githubFetchBranchPRs.mockResolvedValue({})
  })

  it('does not invalidate the CLI fallback for ordinary or debounced branch-list loads', async () => {
    const repoRoot = 'C:/repo'
    const branch = 'feature/large-pr'

    await loadBranchPRs(repoRoot)
    await loadBranchPRs(repoRoot)

    expect(api.githubFetchBranchPRs).toHaveBeenCalledOnce()
    expect(getPRFallbackGeneration(repoRoot, branch)).toBe(0)

    invalidatePRFallback(repoRoot, branch)

    expect(getPRFallbackGeneration(repoRoot, branch)).toBe(1)
    expect(getPRFallbackGeneration(repoRoot, 'another-branch')).toBe(0)
  })

  it('coalesces identical summary reads and starts a fresh read after invalidation', async () => {
    const first = Promise.withResolvers<{
      number: number
      state: string
      isDraft: boolean
    } | null>()
    const second = Promise.withResolvers<{
      number: number
      state: string
      isDraft: boolean
    } | null>()
    api.taskTrackerPRSummary.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)

    const a = loadPRFallbackSummary('C:\\repo', 'feature/large-pr')
    const b = loadPRFallbackSummary('C:/repo', 'feature/large-pr')

    expect(a).toBe(b)
    expect(api.taskTrackerPRSummary).toHaveBeenCalledOnce()

    invalidatePRFallback('C:/repo', 'feature/large-pr')
    const refreshed = loadPRFallbackSummary('C:/repo', 'feature/large-pr')

    expect(refreshed).not.toBe(a)
    expect(api.taskTrackerPRSummary).toHaveBeenCalledTimes(2)

    first.resolve({ number: 344, state: 'OPEN', isDraft: false })
    second.resolve({ number: 344, state: 'MERGED', isDraft: false })

    await expect(a).resolves.toMatchObject({ state: 'OPEN' })
    await expect(refreshed).resolves.toMatchObject({ state: 'MERGED' })

    const cached = loadPRFallbackSummary('C:/repo', 'feature/large-pr')
    expect(cached).toBe(refreshed)
    await expect(cached).resolves.toMatchObject({ state: 'MERGED' })
    expect(api.taskTrackerPRSummary).toHaveBeenCalledTimes(2)
  })

  it('refreshes a settled summary after its short cache lifetime', async () => {
    let now = 10_000
    vi.spyOn(Date, 'now').mockImplementation(() => now)
    api.taskTrackerPRSummary.mockResolvedValue(null)

    const first = loadPRFallbackSummary('C:/repo', 'feature/external-pr')
    await expect(first).resolves.toBeNull()

    now += PR_FALLBACK_TTL_MS - 1
    expect(loadPRFallbackSummary('C:/repo', 'feature/external-pr')).toBe(first)

    now += 1
    const refreshed = loadPRFallbackSummary('C:/repo', 'feature/external-pr')
    expect(refreshed).not.toBe(first)
    await expect(refreshed).resolves.toBeNull()
    expect(api.taskTrackerPRSummary).toHaveBeenCalledTimes(2)
  })
})
