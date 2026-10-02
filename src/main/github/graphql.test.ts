import { afterEach, describe, expect, it, vi } from 'vitest'
import { graphqlFetch } from './graphql'

describe('graphqlFetch', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('reports a 403 with requests left as an API error, not a rate limit', async () => {
    // GitHub sends x-ratelimit-reset on every response, including SSO / scope denials.
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response('Resource protected by organization SAML enforcement', {
            status: 403,
            headers: { 'x-ratelimit-remaining': '4999', 'x-ratelimit-reset': '1800000000' },
          }),
      ),
    )

    const result = await graphqlFetch('https://api.github.com/graphql', 'token', 'query { x }')

    expect(result.isErr() && result.error).toEqual({
      _tag: 'GitHubApiError',
      status: 403,
      message: 'Resource protected by organization SAML enforcement',
    })
  })

  it('reports an exhausted primary window as a rate limit', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(null, {
            status: 403,
            headers: { 'x-ratelimit-remaining': '0', 'x-ratelimit-reset': '1800000000' },
          }),
      ),
    )

    const result = await graphqlFetch('https://api.github.com/graphql', 'token', 'query { x }')

    expect(result.isErr() && result.error).toEqual({
      _tag: 'GitHubRateLimited',
      resetAt: 1_800_000_000,
    })
  })
})
