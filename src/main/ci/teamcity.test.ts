import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  activityBuildTypesLocator,
  buildBranchLocator,
  fetchActivity,
  fetchBuildForBranch,
  fetchBuildTypes,
  isTeamCityLocatorSafeRef,
  mapBuild,
  parseBuildsResponse,
  queuedActivityLocator,
  selectTeamCityConnectionAddress,
  testConnection,
  triggerBuild,
  validateTeamCityConnectionAddresses,
} from './teamcity'
import { ciErrorMessage } from './errors'

const PUBLIC_CONNECTION = { allowPrivate: false }

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('activityBuildTypesLocator', () => {
  it('builds a union scoped to every configured build type', () => {
    expect(activityBuildTypesLocator(['Gakko_Build', 'Gakko_Deploy'])).toBe(
      'buildType:(item:(id:Gakko_Build),item:(id:Gakko_Deploy))',
    )
  })

  it('keeps queued activity scoped and bounded with queue-supported dimensions', () => {
    expect(queuedActivityLocator(['Gakko_Build'])).toBe(
      'buildType:(item:(id:Gakko_Build)),count:20',
    )
  })
})

describe('TeamCity socket address guard', () => {
  const mixedAddresses = [
    { address: '203.0.113.10', family: 4 },
    { address: '127.0.0.1', family: 4 },
  ]

  it('rejects a private or mixed actual lookup without exact-origin consent', () => {
    expect(() => selectTeamCityConnectionAddress(mixedAddresses, PUBLIC_CONNECTION)).toThrow(
      'private network',
    )
  })

  it('allows the same actual lookup after exact-origin consent', () => {
    expect(selectTeamCityConnectionAddress(mixedAddresses, { allowPrivate: true })).toEqual(
      mixedAddresses[0],
    )
  })

  it('preserves every validated address for Undici fallback and Happy Eyeballs', () => {
    expect(validateTeamCityConnectionAddresses(mixedAddresses, { allowPrivate: true })).toEqual(
      mixedAddresses,
    )
  })

  it('blocks a private IP literal before the request without consent', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    const result = await testConnection('http://127.0.0.1:8111', 'token', PUBLIC_CONNECTION)

    expect(result.isErr() && result.error._tag).toBe('CiPrivateOriginApprovalRequired')
    expect(fetchMock).not.toHaveBeenCalled()
  })
})

describe('triggerBuild ambiguity', () => {
  it('reports a 502 after dispatch as ambiguous and performs one request', async () => {
    const response = new Response(JSON.stringify({ message: 'upstream failed' }), {
      status: 502,
      headers: { 'Content-Type': 'application/json' },
    })
    const fetchMock = vi.fn().mockResolvedValue(response)
    vi.stubGlobal('fetch', fetchMock)

    const result = await triggerBuild(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      'Deploy',
      'next',
    )

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiDispatchAmbiguous',
      provider: 'teamcity',
    })
    expect(fetchMock).toHaveBeenCalledOnce()
    expect(response.bodyUsed).toBe(true)
  })

  it('reports a transport failure after dispatch as ambiguous and performs one request', async () => {
    const fetchMock = vi.fn().mockRejectedValue(new Error('connection reset'))
    vi.stubGlobal('fetch', fetchMock)

    const result = await triggerBuild(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      'Deploy',
      'next',
    )

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiDispatchAmbiguous',
      provider: 'teamcity',
    })
    expect(fetchMock).toHaveBeenCalledOnce()
  })

  it('cancels an oversized 502 body while preserving an ambiguous dispatch result', async () => {
    const response = new Response('upstream failed', {
      status: 502,
      headers: { 'Content-Length': String(2 * 1024 * 1024 + 1) },
    })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response))

    const result = await triggerBuild(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      'Deploy',
      'next',
    )

    expect(result.isErr() && result.error._tag).toBe('CiDispatchAmbiguous')
    expect(response.bodyUsed).toBe(true)
  })

  it('keeps a definite 4xx rejection retryable', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ message: 'invalid branch' }), {
          status: 400,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    )

    const result = await triggerBuild(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      'Deploy',
      'next',
    )

    expect(result.isErr() && result.error).toMatchObject({ _tag: 'CiApiError', status: 400 })
  })

  it('does not reflect submitted parameter secrets from a mutation error', async () => {
    const secret = 'deployment-password'
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(`Invalid value ${secret}`, { status: 400 })),
    )

    const result = await triggerBuild(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      'Deploy',
      'next',
      [{ name: 'password', value: secret }],
    )

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiApiError',
      status: 400,
      message: 'TeamCity rejected the request',
    })
    expect(result.isErr() && JSON.stringify(result.error)).not.toContain(secret)
  })
})

describe('fetchActivity', () => {
  it('keeps running and recent builds when the queue query fails', async () => {
    const response = (body: unknown, status = 200): Response =>
      new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
      })
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(
          response({ build: [{ id: 1, state: 'running', buildType: { id: 'Build' } }] }),
        )
        .mockResolvedValueOnce(response({ message: 'queue forbidden' }, 403))
        .mockResolvedValueOnce(
          response({ build: [{ id: 2, state: 'finished', buildType: { id: 'Build' } }] }),
        ),
    )

    const result = await fetchActivity('https://tc.example.com', 'token', PUBLIC_CONNECTION, [
      'Build',
    ])

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value.running.map((build) => build.id)).toEqual([1])
    expect(result.value.queued).toEqual([])
    expect(result.value.recent.map((build) => build.id)).toEqual([2])
    expect(result.value.partialErrors).toEqual([
      expect.stringContaining('Queued builds: TeamCity API error 403'),
    ])
  })

  it('states one shared reason once when all three queries fail the same way', async () => {
    // A rejected token fails every slice identically. Joining the three sentences under a
    // wrapper that repeats the status put the same line in front of the user four times.
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('VPN unavailable')))

    const result = await fetchActivity('https://tc.example.com', 'token', PUBLIC_CONNECTION, [
      'Build',
    ])

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected total activity failure')
    expect(result.error).toMatchObject({ message: 'VPN unavailable' })
    expect(ciErrorMessage(result.error)).not.toMatch(/VPN unavailable[\s\S]*VPN unavailable/)
  })

  it('still names the slices when they failed for DIFFERENT reasons', async () => {
    const body = (message: string, status: number): Response =>
      new Response(JSON.stringify({ message }), {
        status,
        headers: { 'Content-Type': 'application/json' },
      })
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValueOnce(body('nope', 401))
        .mockResolvedValueOnce(body('gone', 404))
        .mockResolvedValueOnce(body('boom', 500)),
    )

    const result = await fetchActivity('https://tc.example.com', 'token', PUBLIC_CONNECTION, [
      'Build',
    ])

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected total activity failure')
    const message = (result.error as { message: string }).message
    expect(message).toContain('Running builds:')
    expect(message).toContain('Queued builds:')
    expect(message).toContain('Recent builds:')
  })

  it('narrows running and recent SERVER-side, and the unfilterable queue in the response', async () => {
    // The whole point of the branch filter: `count:10` is applied by TeamCity, so a
    // response-side filter would return nothing for a branch whose builds are older.
    // A fresh Response per call: a body can only be consumed once, so a shared
    // instance would leave the second and third slices empty for the wrong reason.
    const fetchMock = vi.fn().mockImplementation(
      async () =>
        new Response(
          JSON.stringify({
            build: [
              { id: 1, branchName: 'feat/x', buildType: { id: 'Build' } },
              { id: 2, branchName: 'main', buildType: { id: 'Build' } },
            ],
          }),
          { status: 200, headers: { 'Content-Type': 'application/json' } },
        ),
    )
    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchActivity(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      ['Build'],
      'feat/x',
    )

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    const urls = fetchMock.mock.calls.map(([url]) => decodeURIComponent(String(url)))
    expect(urls[0]).toContain('branch:(name:(feat/x))')
    expect(urls[2]).toContain('branch:(name:(feat/x))')
    expect(urls.join(' ')).not.toContain('branch:(default:any)')
    // BuildQueueLocator has no `branch` dimension, so that slice alone is filtered here.
    expect(urls[1]).not.toContain('branch:')
    expect(result.value.queued.map((build) => build.id)).toEqual([1])
    // The two server-filtered slices are trusted as returned — no second filter pass.
    expect(result.value.running.map((build) => build.id)).toEqual([1, 2])
  })

  it('refuses a branch that would escape the locator parentheses', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchActivity(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      ['Build'],
      'feat/x),foo:(',
    )

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected a rejected branch')
    expect(result.error).toMatchObject({
      _tag: 'CiApiError',
      message: 'TeamCity branch contains locator-unsafe characters',
    })
    expect(fetchMock).not.toHaveBeenCalled()
  })
})

describe('testConnection', () => {
  it('requires explicit TeamCity server identity metadata', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 200 })))

    const result = await testConnection('https://tc.example.com', 'token', PUBLIC_CONNECTION)

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiApiError',
      message: 'TeamCity returned an invalid response shape',
    })
  })

  it('rejects an empty successful response instead of treating it as a connection success', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 200 })))

    const result = await testConnection('https://tc.example.com', 'token', PUBLIC_CONNECTION)

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiApiError',
      message: 'TeamCity returned malformed JSON',
    })
  })

  it('returns a structured error when the build-type endpoint has an empty success body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('', { status: 200 })))

    const result = await fetchBuildTypes('https://tc.example.com', 'token', PUBLIC_CONNECTION)

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiApiError',
      message: 'TeamCity returned malformed JSON',
    })
  })

  it('returns a structured error when a successful build-type response is not an array', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ buildType: {} }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    )

    const result = await fetchBuildTypes('https://tc.example.com', 'token', PUBLIC_CONNECTION)

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiApiError',
      message: 'TeamCity returned an invalid response shape',
    })
  })

  it('rejects a response whose declared size exceeds the shared API limit', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('{"version":"1"}', {
          headers: { 'Content-Length': String(2 * 1024 * 1024 + 1) },
        }),
      ),
    )

    const result = await testConnection('https://tc.example.com', 'token', PUBLIC_CONNECTION)

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected an oversized response to fail')
    expect(result.error).toMatchObject({
      _tag: 'CiApiError',
      message: 'TeamCity response exceeds the size limit',
    })
  })

  it('rejects an oversized response when Content-Length is absent', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(new Response(JSON.stringify({ padding: 'x'.repeat(2 * 1024 * 1024) }))),
    )

    const result = await testConnection('https://tc.example.com', 'token', PUBLIC_CONNECTION)

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected an oversized response to fail')
    expect(result.error).toMatchObject({
      _tag: 'CiApiError',
      message: 'TeamCity response exceeds the size limit',
    })
  })

  it('preserves an authentication status when its response body is oversized', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('unauthorized', {
          status: 401,
          headers: { 'Content-Length': String(2 * 1024 * 1024 + 1) },
        }),
      ),
    )

    const result = await testConnection('https://tc.example.com', 'token', PUBLIC_CONNECTION)

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiApiError',
      status: 401,
      message: 'TeamCity response exceeds the size limit',
    })
  })

  it('redacts a token echoed by a TeamCity error response', async () => {
    const token = 'secret-token-value'
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(`Bearer ${token}`, { status: 401 })),
    )

    const result = await testConnection('https://tc.example.com', token, PUBLIC_CONNECTION)

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected the connection to fail')
    expect(result.error).toMatchObject({
      _tag: 'CiApiError',
      status: 401,
      message: expect.stringContaining('[redacted]'),
    })
    expect(result.error._tag === 'CiApiError' && result.error.message).not.toContain(token)
  })

  it('redacts a token echoed by a network exception', async () => {
    const token = 'secret-token-value'
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error(`request included ${token}`)))

    const result = await testConnection('https://tc.example.com', token, PUBLIC_CONNECTION)

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected the connection to fail')
    expect(result.error._tag === 'CiApiError' && result.error.message).toContain('[redacted]')
    expect(result.error._tag === 'CiApiError' && result.error.message).not.toContain(token)
  })
})

describe('buildBranchLocator', () => {
  it('scopes to build type and branch, including queued and running builds', () => {
    expect(buildBranchLocator('Gakko_Build', 's152/ISSUE-2148')).toBe(
      'buildType:(id:Gakko_Build),branch:(name:(s152/ISSUE-2148)),running:any,defaultFilter:false,count:1',
    )
  })

  it('accepts non-ASCII refs but rejects locator structural characters', () => {
    expect(isTeamCityLocatorSafeRef('feature/ąę')).toBe(true)
    expect(isTeamCityLocatorSafeRef('feat(ci),v2')).toBe(false)
  })
})

describe('mapBuild', () => {
  it('maps a finished successful build', () => {
    expect(
      mapBuild({
        id: 123,
        number: '45',
        state: 'finished',
        status: 'SUCCESS',
        webUrl: 'https://tc/build/123',
        branchName: 's152/ISSUE-2148',
        queuedDate: '20260801T172200+0200',
        startDate: '20260801T172255+0200',
        finishDate: '20260801T172406+0200',
      }),
    ).toEqual({
      id: 123,
      number: '45',
      state: 'finished',
      status: 'SUCCESS',
      statusText: undefined,
      percentageComplete: undefined,
      webUrl: 'https://tc/build/123',
      branchName: 's152/ISSUE-2148',
      queuedAt: Date.parse('2026-08-01T17:22:00+02:00'),
      startedAt: Date.parse('2026-08-01T17:22:55+02:00'),
      finishedAt: Date.parse('2026-08-01T17:24:06+02:00'),
    })
  })

  it('maps a running build with progress', () => {
    const mapped = mapBuild({
      id: 7,
      number: '46',
      state: 'running',
      status: 'SUCCESS',
      statusText: 'Step 3/7',
      percentageComplete: 42,
      webUrl: 'https://tc/build/7',
    })
    expect(mapped.state).toBe('running')
    expect(mapped.statusText).toBe('Step 3/7')
    expect(mapped.percentageComplete).toBe(42)
    expect(mapped.branchName).toBeUndefined()
  })

  it('normalizes unknown states and statuses defensively', () => {
    const mapped = mapBuild({
      id: 9,
      number: '1',
      state: 'deleting',
      status: 'WEIRD',
      webUrl: 'https://tc/build/9',
    })
    expect(mapped.state).toBe('finished')
    expect(mapped.status).toBe('UNKNOWN')
  })

  it('keeps ERROR distinct from UNKNOWN', () => {
    // ERROR is TeamCity's infra/agent failure — folding it into UNKNOWN would give
    // an infra-failed build the neutral chip and the neutral completion toast,
    // while the activity window (raw passthrough) would still call it Failed.
    const mapped = mapBuild({
      id: 10,
      number: '3',
      state: 'finished',
      status: 'ERROR',
      webUrl: 'https://tc/build/10',
    })
    expect(mapped.status).toBe('ERROR')
  })

  it('treats a queued build without status as unknown status', () => {
    const mapped = mapBuild({ id: 5, number: '2', state: 'queued', webUrl: 'https://tc/q/5' })
    expect(mapped.state).toBe('queued')
    expect(mapped.status).toBe('UNKNOWN')
  })
})

describe('parseBuildsResponse', () => {
  it('returns null when the branch has no builds', () => {
    expect(parseBuildsResponse({ count: 0, build: [] })).toBeNull()
    expect(parseBuildsResponse({})).toBeNull()
  })

  it('returns the first (newest) build mapped', () => {
    const parsed = parseBuildsResponse({
      count: 2,
      build: [
        {
          id: 2,
          number: '2',
          state: 'running',
          statusText: 'Compiling',
          webUrl: 'https://tc/2',
        },
        { id: 1, number: '1', state: 'finished', status: 'FAILURE', webUrl: 'https://tc/1' },
      ],
    })
    expect(parsed?.id).toBe(2)
    expect(parsed?.state).toBe('running')
    expect(parsed?.statusText).toBe('Compiling')
  })
})

describe('fetchBuildForBranch', () => {
  it('requests and preserves TeamCity statusText for the last-job card', async () => {
    const fetchMock = vi.fn<typeof fetch>(async (input) => {
      expect(String(input)).toContain('statusText')
      return new Response(
        JSON.stringify({
          count: 1,
          build: [
            {
              id: 42,
              number: '2624',
              state: 'finished',
              status: 'SUCCESS',
              statusText: 'kadry-backend-test deployed',
            },
          ],
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      )
    })
    vi.stubGlobal('fetch', fetchMock)

    const result = await fetchBuildForBranch(
      'https://tc.example.com',
      'token',
      PUBLIC_CONNECTION,
      'Gakko_Build',
      'feature/status',
    )

    expect(result.isOk() && result.value?.statusText).toBe('kadry-backend-test deployed')
  })
})
