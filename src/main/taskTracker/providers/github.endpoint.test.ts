import { errAsync, okAsync } from 'neverthrow'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const graphqlFetch = vi.hoisted(() => vi.fn())
vi.mock('../../github/graphql', () => ({ graphqlFetch }))

import { githubClient } from './github'
import type { TaskTrackerConnection } from '../types'

function connection(baseUrl: string): TaskTrackerConnection {
  return {
    id: 'gh',
    provider: 'github',
    name: 'GitHub',
    baseUrl,
    projectKey: 'itsoltech/canopy-desktop',
    authPrefKey: 'k',
  }
}

describe('GitHub tracker endpoint and errors', () => {
  beforeEach(() => graphqlFetch.mockReset())

  it('sends GitHub Enterprise requests to the configured port', async () => {
    graphqlFetch.mockReturnValue(okAsync({ viewer: { login: 'octo' } }))
    await githubClient.testConnection(connection('https://ghe.example.com:8443'), 't')
    expect(graphqlFetch.mock.calls[0][0]).toBe('https://ghe.example.com:8443/api/graphql')
  })

  it('keeps github.com on the public API host', async () => {
    graphqlFetch.mockReturnValue(okAsync({ viewer: { login: 'octo' } }))
    await githubClient.testConnection(connection('https://github.com'), 't')
    expect(graphqlFetch.mock.calls[0][0]).toBe('https://api.github.com/graphql')
  })

  it('reports a rate limit as a rate limit, not as an unknown error', async () => {
    graphqlFetch.mockReturnValue(errAsync({ _tag: 'GitHubRateLimited', resetAt: 1_700_000_000 }))
    const result = await githubClient.testConnection(connection('https://github.com'), 't')
    expect(result._unsafeUnwrapErr()).toMatchObject({
      _tag: 'ProviderApiError',
      message: expect.stringContaining('rate limit'),
    })
  })

  it('keeps the HTTP status and message of API errors', async () => {
    graphqlFetch.mockReturnValue(
      errAsync({ _tag: 'GitHubApiError', status: 401, message: 'Bad credentials' }),
    )
    const result = await githubClient.testConnection(connection('https://github.com'), 't')
    expect(result._unsafeUnwrapErr()).toEqual({
      _tag: 'ProviderApiError',
      status: 401,
      message: 'Bad credentials',
      provider: 'github',
    })
  })
})
