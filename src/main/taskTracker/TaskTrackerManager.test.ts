import { afterEach, describe, expect, it, vi } from 'vitest'
import { ok, okAsync, type ResultAsync } from 'neverthrow'
import type { PreferencesStore } from '../db/PreferencesStore'
import { GitRepository } from '../git/GitRepository'
import { defaultConfig } from './configDefaults'
import type { TaskTrackerError } from './errors'
import type { KeychainTokenStore } from './KeychainTokenStore'
import { githubClient } from './providers/github'
import { TaskTrackerManager } from './TaskTrackerManager'
import type { RepoConfig, TrackerBoard } from './types'

afterEach(() => {
  vi.restoreAllMocks()
})

// Only the two credential lookups the config-based fetch path performs.
const keychain = {
  getCredentialsForTracker: () => ({ token: 'ghp_secret' }),
  getCredentialsForTrackerResult: () => ok({ token: 'ghp_secret' }),
} as unknown as KeychainTokenStore

function githubConfig(baseUrl: string): RepoConfig {
  return { ...defaultConfig(), trackers: [{ id: 'gh', provider: 'github', baseUrl }] }
}

function fetchBoardsWithRemote(
  baseUrl: string,
  remoteUrl: string,
): ResultAsync<TrackerBoard[], TaskTrackerError> {
  vi.spyOn(GitRepository, 'getRemoteUrl').mockReturnValue(okAsync(remoteUrl))
  vi.spyOn(githubClient, 'fetchBoards').mockReturnValue(okAsync([]))
  const manager = new TaskTrackerManager({} as PreferencesStore, keychain)
  return manager.fetchBoardsFromConfig(githubConfig(baseUrl), undefined, '/repo')
}

describe('GitHub tracker auto-detection from the workspace remote', () => {
  it('takes owner/repo from a remote on the tracker host', async () => {
    const result = await fetchBoardsWithRemote('', 'git@github.com:owner/repo.git')

    expect(result.isOk()).toBe(true)
    expect(githubClient.fetchBoards).toHaveBeenCalledWith(
      expect.objectContaining({ baseUrl: 'https://github.com', projectKey: 'owner/repo' }),
      'ghp_secret',
    )
  })

  it('accepts a GitHub Enterprise remote on the configured tracker host', async () => {
    const result = await fetchBoardsWithRemote(
      'https://ghe.example.com',
      'ssh://git@ghe.example.com:2222/owner/repo.git',
    )

    expect(result.isOk()).toBe(true)
    expect(githubClient.fetchBoards).toHaveBeenCalledWith(
      expect.objectContaining({ baseUrl: 'https://ghe.example.com', projectKey: 'owner/repo' }),
      'ghp_secret',
    )
  })

  it.each([
    ['', 'https://gitlab.com/owner/repo.git'],
    ['', 'https://evil.example/owner/repo.git'],
    ['https://ghe.example.com', 'git@github.com:owner/repo.git'],
  ])(
    'never sends the token to a remote host other than the tracker host (%s, %s)',
    async (baseUrl, remoteUrl) => {
      const result = await fetchBoardsWithRemote(baseUrl, remoteUrl)

      expect(result._unsafeUnwrapErr()).toMatchObject({
        _tag: 'ProviderApiError',
        provider: 'github',
      })
      expect(githubClient.fetchBoards).not.toHaveBeenCalled()
    },
  )
})
