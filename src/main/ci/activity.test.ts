import { describe, expect, it } from 'vitest'
import { parseActivity, parseBranches, parseTcDate } from './activity'

describe('parseTcDate', () => {
  it('parses the TeamCity timestamp format with offset', () => {
    expect(parseTcDate('20260801T172347+0200')).toBe(Date.parse('2026-08-01T17:23:47+02:00'))
    expect(parseTcDate('20260801T152347Z')).toBe(Date.parse('2026-08-01T15:23:47Z'))
  })

  it('returns undefined for missing or malformed input', () => {
    expect(parseTcDate(undefined)).toBeUndefined()
    expect(parseTcDate('yesterday')).toBeUndefined()
  })
})

describe('parseActivity', () => {
  it('maps running builds with their configuration and progress', () => {
    const activity = parseActivity(
      {
        count: 1,
        build: [
          {
            id: 55,
            number: '812',
            state: 'running',
            status: 'SUCCESS',
            percentageComplete: 63,
            webUrl: 'https://tc/build/55',
            branchName: 's152/ISSUE-2148',
            buildType: { id: 'Gakko_Build', name: 'Build & Deploy' },
          },
        ],
      },
      { count: 0 },
      {},
    )
    expect(activity.running).toEqual([
      {
        id: 55,
        number: '812',
        state: 'running',
        status: 'SUCCESS',
        percentageComplete: 63,
        webUrl: 'https://tc/build/55',
        branchName: 's152/ISSUE-2148',
        buildTypeId: 'Gakko_Build',
        buildTypeName: 'Build & Deploy',
      },
    ])
    expect(activity.queued).toEqual([])
    expect(activity.recent).toEqual([])
  })

  it('maps queued builds and tolerates missing optional fields', () => {
    const activity = parseActivity(
      {},
      {
        count: 2,
        build: [
          { id: 60, webUrl: 'https://tc/q/60', buildType: { id: 'X' } },
          { id: 61, branchName: 'next' },
        ],
      },
      {},
    )
    expect(activity.queued).toEqual([
      {
        id: 60,
        number: undefined,
        state: 'queued',
        status: undefined,
        percentageComplete: undefined,
        webUrl: 'https://tc/q/60',
        branchName: undefined,
        buildTypeId: 'X',
        buildTypeName: 'X',
      },
      {
        id: 61,
        number: undefined,
        state: 'queued',
        status: undefined,
        percentageComplete: undefined,
        webUrl: '',
        branchName: 'next',
        buildTypeId: '',
        buildTypeName: '',
      },
    ])
  })

  it('maps recent finished builds with their outcome and timestamps', () => {
    const activity = parseActivity(
      {},
      {},
      {
        count: 2,
        build: [
          {
            id: 50,
            number: '2602',
            status: 'SUCCESS',
            webUrl: 'https://tc/build/50',
            branchName: 'develop',
            startDate: '20260801T172255+0200',
            finishDate: '20260801T172406+0200',
            buildType: { id: 'Gakko_Build', name: 'Build & Deploy' },
          },
          { id: 49, number: '2601', status: 'FAILURE', buildType: { id: 'Gakko_Build' } },
        ],
      },
    )
    expect(activity.recent.map((b) => [b.state, b.status, b.number])).toEqual([
      ['finished', 'SUCCESS', '2602'],
      ['finished', 'FAILURE', '2601'],
    ])
    const first = activity.recent[0]
    expect(first.startedAt).toBe(Date.parse('2026-08-01T17:22:55+02:00'))
    expect(first.finishedAt).toBe(Date.parse('2026-08-01T17:24:06+02:00'))
    expect(activity.recent[1].startedAt).toBeUndefined()
  })
})

describe('parseBranches', () => {
  it('puts the default branch first and keeps server order otherwise', () => {
    expect(
      parseBranches({
        count: 3,
        branch: [
          { name: 'feat/a' },
          { name: 'master', default: true },
          { name: 's152/ISSUE-2148' },
        ],
      }),
    ).toEqual(['master', 'feat/a', 's152/ISSUE-2148'])
  })

  it('drops entries without a name and handles empty input', () => {
    expect(parseBranches({ branch: [{ name: '' }, {}] })).toEqual([])
    expect(parseBranches({})).toEqual([])
  })
})
