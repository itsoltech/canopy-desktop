import { afterEach, describe, expect, it, vi } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { TaskTrackerManager } from '../taskTracker/TaskTrackerManager'
import { GitHubService } from './GitHubService'

const json = (data: unknown): Response =>
  new Response(JSON.stringify({ data }), { headers: { 'Content-Type': 'application/json' } })

describe('GitHubService.fetchOpenPRsForBranches', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it("ignores other people's fork PRs that reuse a local branch name", async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        json({
          viewer: { login: 'me' },
          search: {
            nodes: [
              {
                number: 7,
                headRefName: 'main',
                isCrossRepository: true,
                headRepositoryOwner: { login: 'contributor' },
              },
              {
                number: 8,
                headRefName: 'fix-typo',
                isCrossRepository: true,
                headRepositoryOwner: { login: 'me' },
              },
              {
                number: 9,
                headRefName: 'feature',
                isCrossRepository: false,
                headRepositoryOwner: { login: 'itsoltech' },
              },
            ],
          },
        }),
      ),
    )
    // fetchOpenPRsForBranches only talks to the API it is given.
    const service = new GitHubService({} as PreferencesStore, {} as TaskTrackerManager)

    const result = await service.fetchOpenPRsForBranches(
      'https://api.github.com/graphql',
      'token',
      'itsoltech',
      'canopy-desktop',
      ['main', 'fix-typo', 'feature'],
    )

    if (result.isErr()) throw new Error(JSON.stringify(result.error))
    expect(Object.keys(result.value).sort()).toEqual(['feature', 'fix-typo'])
  })
})
