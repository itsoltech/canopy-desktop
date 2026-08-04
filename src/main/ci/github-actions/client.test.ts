import { afterEach, describe, expect, it, vi } from 'vitest'
import { GitHubActionsClient } from './client'

const jsonResponse = (body: unknown, init: ResponseInit = {}): Response =>
  new Response(JSON.stringify(body), {
    status: init.status ?? 200,
    headers: { 'Content-Type': 'application/json', ...(init.headers ?? {}) },
  })

describe('GitHubActionsClient', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('pins the API version, confines the repository and refuses redirects', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () => jsonResponse({ workflows: [] }))
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'secret-token')

    const result = await client.listWorkflows()

    expect(result.isOk()).toBe(true)
    expect(fetchMock).toHaveBeenCalledOnce()
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe(
      'https://api.github.com/repos/itsoltech/canopy-desktop/actions/workflows?per_page=100&page=1',
    )
    if (!init) throw new Error('Expected fetch init')
    expect(init).toMatchObject({ redirect: 'error' })
    expect(init.headers).toMatchObject({
      Accept: 'application/vnd.github+json',
      Authorization: 'Bearer secret-token',
      'X-GitHub-Api-Version': '2026-03-10',
    })
  })

  it('loads a selected-ref workflow with its blob SHA and decoded content', async () => {
    const source = 'on: workflow_dispatch\n'
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        jsonResponse({ sha: 'blob-sha', encoding: 'base64', content: btoa(source) }),
      ),
    )
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'token')

    const result = await client.getWorkflowFile('.github/workflows/release.yml', 'feature/x')

    expect(result.isOk() && result.value).toEqual({ sha: 'blob-sha', content: source })
    expect(vi.mocked(fetch).mock.calls[0]?.[0]).toContain(
      '/contents/.github/workflows/release.yml?ref=feature%2Fx',
    )
  })

  it('loads repository metadata through a read-only repository endpoint', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () =>
      jsonResponse({ full_name: 'itsoltech/canopy-desktop', default_branch: 'next' }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'token')

    const result = await client.getRepository()

    expect(result.isOk() && result.value).toEqual({
      fullName: 'itsoltech/canopy-desktop',
      defaultBranch: 'next',
    })
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      'https://api.github.com/repos/itsoltech/canopy-desktop',
    )
  })

  it('paginates refs but stops at the bounded five-page limit', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () =>
      jsonResponse(
        Array.from({ length: 100 }, (_, index) => ({
          name: `branch-${index}`,
          commit: { sha: `sha-${index}` },
        })),
      ),
    )
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'token')

    const result = await client.listBranches()

    expect(result.isOk() && result.value).toHaveLength(500)
    expect(fetchMock).toHaveBeenCalledTimes(5)
    expect(fetchMock.mock.calls[4]?.[0]).toContain('/branches?per_page=100&page=5')
  })

  it('resolves an exact typed ref without relying on bounded picker pages', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () =>
      jsonResponse({ ref: 'refs/heads/feature/x', object: { sha: 'exact-sha' } }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'token')

    const result = await client.getExactRef('branch', 'feature/x')

    expect(result.isOk() && result.value).toEqual({ name: 'feature/x', commitSha: 'exact-sha' })
    expect(fetchMock.mock.calls[0]?.[0]).toContain('/git/ref/heads/feature%2Fx')
  })

  it('paginates environments until the reported total is loaded', async () => {
    const fetchMock = vi.fn<typeof fetch>(async (url) => {
      const page = new URL(String(url)).searchParams.get('page')
      return jsonResponse({
        total_count: 101,
        environments:
          page === '1'
            ? Array.from({ length: 100 }, (_, index) => ({ name: `environment-${index}` }))
            : [{ name: 'environment-100' }],
      })
    })
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'token')

    const result = await client.listEnvironments()

    expect(result.isOk() && result.value).toHaveLength(101)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('dispatches exactly once and returns GitHub direct run identity', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () =>
      jsonResponse({ workflow_run_id: 123456789012, run_url: 'api-run', html_url: 'web-run' }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'token')

    const result = await client.dispatchWorkflow(42, 'next', { dry_run: true, notes: 'safe' })

    expect(result.isOk() && result.value).toEqual({
      runId: '123456789012',
      apiUrl: 'api-run',
      webUrl: 'web-run',
    })
    expect(fetchMock).toHaveBeenCalledOnce()
    expect(fetchMock.mock.calls[0]?.[1]).toMatchObject({
      method: 'POST',
      body: JSON.stringify({ ref: 'next', inputs: { dry_run: true, notes: 'safe' } }),
    })
  })

  it('treats an accepted response without a run id as ambiguous and never retries', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'token')

    const result = await client.dispatchWorkflow(42, 'next', { dry_run: true })

    expect(result.isErr() && result.error._tag).toBe('CiDispatchAmbiguous')
    expect(fetchMock).toHaveBeenCalledOnce()
  })

  it.each([
    ['transport failure', () => Promise.reject(new Error('socket reset'))],
    ['malformed success body', () => Promise.resolve(new Response('not json', { status: 200 }))],
  ])('treats a %s after dispatch starts as ambiguous and never retries', async (_name, reply) => {
    const fetchMock = vi.fn<typeof fetch>(reply)
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'dispatch-ambiguity', 'token')

    const result = await client.dispatchWorkflow(42, 'next', {})

    expect(result.isErr() && result.error._tag).toBe('CiDispatchAmbiguous')
    expect(fetchMock).toHaveBeenCalledOnce()
  })

  it('honors secondary rate-limit Retry-After responses across client instances', async () => {
    const fetchMock = vi.fn<typeof fetch>(
      async () => new Response(null, { status: 429, headers: { 'retry-after': '60' } }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const first = new GitHubActionsClient('itsoltech', 'secondary-limit', 'token')

    const result = await first.listWorkflows()
    const suppressed = await new GitHubActionsClient(
      'itsoltech',
      'secondary-limit',
      'other-token',
    ).listWorkflows()

    expect(result.isErr() && result.error._tag).toBe('CiRateLimited')
    expect(suppressed.isErr() && suppressed.error._tag).toBe('CiRateLimited')
    expect(fetchMock).toHaveBeenCalledOnce()
  })

  it('maps rate limits without exposing response bodies or tokens', async () => {
    const fetchMock = vi.fn(
      async () =>
        new Response('secret-token should never be reflected', {
          status: 403,
          headers: { 'x-ratelimit-remaining': '0', 'x-ratelimit-reset': '1800000000' },
        }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const client = new GitHubActionsClient('itsoltech', 'canopy-desktop', 'secret-token')

    const result = await client.listWorkflows()
    const suppressed = await new GitHubActionsClient(
      'itsoltech',
      'canopy-desktop',
      'another-token',
    ).listWorkflows()

    expect(result.isErr() && result.error).toEqual({
      _tag: 'CiRateLimited',
      resetAt: 1_800_000_000_000,
    })
    expect(suppressed.isErr() && suppressed.error).toEqual({
      _tag: 'CiRateLimited',
      resetAt: 1_800_000_000_000,
    })
    expect(fetchMock).toHaveBeenCalledOnce()
  })
})
