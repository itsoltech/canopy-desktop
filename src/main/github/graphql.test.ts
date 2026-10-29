import { afterEach, describe, expect, it, vi } from 'vitest'
import { graphqlFetch } from './graphql'

const API_URL = 'https://api.github.com/graphql'
const QUERY = 'query { viewer { login } }'

function stubForbidden(headers: Record<string, string>): void {
  vi.stubGlobal(
    'fetch',
    vi.fn<typeof fetch>(
      async () =>
        new Response('Resource protected by organization SAML enforcement', {
          status: 403,
          headers,
        }),
    ),
  )
}

describe('graphqlFetch', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('reports a 403 with quota left as an API error, not a rate limit', async () => {
    stubForbidden({ 'x-ratelimit-remaining': '4999', 'x-ratelimit-reset': '1700000000' })

    const result = await graphqlFetch(API_URL, 'token', QUERY)

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'GitHubApiError',
      status: 403,
    })
  })

  it('reports a 403 with an exhausted quota as a rate limit', async () => {
    stubForbidden({ 'x-ratelimit-remaining': '0', 'x-ratelimit-reset': '1700000000' })

    const result = await graphqlFetch(API_URL, 'token', QUERY)

    expect(result.isErr() && result.error).toEqual({
      _tag: 'GitHubRateLimited',
      resetAt: 1700000000,
    })
  })
})
