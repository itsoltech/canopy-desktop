import { describe, expect, it } from 'vitest'
import {
  activityBuildTypesLocator,
  buildBranchLocator,
  mapBuild,
  parseBuildsResponse,
} from './teamcity'

describe('activityBuildTypesLocator', () => {
  it('builds a union scoped to every configured build type', () => {
    expect(activityBuildTypesLocator(['Gakko_Build', 'Gakko_Deploy'])).toBe(
      'buildType:(item:(id:Gakko_Build),item:(id:Gakko_Deploy))',
    )
  })
})

describe('buildBranchLocator', () => {
  it('scopes to build type and branch, including queued and running builds', () => {
    expect(buildBranchLocator('Gakko_Build', 's152/ISSUE-2148')).toBe(
      'buildType:(id:Gakko_Build),branch:(name:(s152/ISSUE-2148)),running:any,defaultFilter:false,count:1',
    )
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
      }),
    ).toEqual({
      id: 123,
      number: '45',
      state: 'finished',
      status: 'SUCCESS',
      percentageComplete: undefined,
      webUrl: 'https://tc/build/123',
      branchName: 's152/ISSUE-2148',
    })
  })

  it('maps a running build with progress', () => {
    const mapped = mapBuild({
      id: 7,
      number: '46',
      state: 'running',
      status: 'SUCCESS',
      percentageComplete: 42,
      webUrl: 'https://tc/build/7',
    })
    expect(mapped.state).toBe('running')
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
        { id: 2, number: '2', state: 'running', webUrl: 'https://tc/2' },
        { id: 1, number: '1', state: 'finished', status: 'FAILURE', webUrl: 'https://tc/1' },
      ],
    })
    expect(parsed?.id).toBe(2)
    expect(parsed?.state).toBe('running')
  })
})
