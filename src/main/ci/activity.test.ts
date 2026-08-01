import { describe, expect, it } from 'vitest'
import { parseActivity, parseBranches } from './activity'

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

  it('maps recent finished builds with their outcome', () => {
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
