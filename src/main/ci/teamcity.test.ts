import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  activityBuildTypesLocator,
  buildBranchLocator,
  fetchActivity,
  fetchBuildForBranch,
  isTeamCityLocatorSafeRef,
  mapBuild,
  parseBuildsResponse,
  queuedActivityLocator,
} from './teamcity'

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

    const result = await fetchActivity('https://tc.example.com', 'token', ['Build'])

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value.running.map((build) => build.id)).toEqual([1])
    expect(result.value.queued).toEqual([])
    expect(result.value.recent.map((build) => build.id)).toEqual([2])
    expect(result.value.partialErrors).toEqual([
      expect.stringContaining('Queued builds: TeamCity API error 403'),
    ])
  })

  it('returns an error when every activity query fails', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('VPN unavailable')))

    const result = await fetchActivity('https://tc.example.com', 'token', ['Build'])

    expect(result.isErr()).toBe(true)
    if (result.isOk()) throw new Error('Expected total activity failure')
    expect(result.error).toMatchObject({
      _tag: 'CiApiError',
      message: expect.stringContaining('Running builds: TeamCity: VPN unavailable'),
    })
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
      'Gakko_Build',
      'feature/status',
    )

    expect(result.isOk() && result.value?.statusText).toBe('kadry-backend-test deployed')
  })
})
