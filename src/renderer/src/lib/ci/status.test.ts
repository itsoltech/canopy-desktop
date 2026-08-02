import { describe, expect, it } from 'vitest'
import { anyBuildActive, ciChip } from './status'

function build(overrides: Partial<CiBuildStatus>): CiBuildStatus {
  return {
    id: 1,
    number: '1',
    state: 'finished',
    status: 'SUCCESS',
    percentageComplete: undefined,
    webUrl: 'https://tc/build/1',
    branchName: undefined,
    ...overrides,
  }
}

describe('ciChip', () => {
  it('labels the empty state', () => {
    expect(ciChip({ build: null }).label).toBe('No builds')
  })

  it('distinguishes a failed fetch from "never built"', () => {
    const chip = ciChip({ build: null, error: 'TeamCity API error 404: not found' })
    expect(chip.label).toBe('Unavailable')
    expect(chip.label).not.toBe('No builds')
  })

  it('shows progress for running builds and omits it when unknown', () => {
    expect(ciChip({ build: build({ state: 'running', percentageComplete: 42 }) }).label).toBe(
      'Running 42%',
    )
    expect(ciChip({ build: build({ state: 'running' }) }).label).toBe('Running')
  })

  it('maps queued and finished outcomes', () => {
    expect(ciChip({ build: build({ state: 'queued', status: 'UNKNOWN' }) }).label).toBe('Queued')
    expect(ciChip({ build: build({ status: 'SUCCESS' }) }).label).toBe('Success')
    expect(ciChip({ build: build({ status: 'FAILURE' }) }).label).toBe('Failed')
    expect(ciChip({ build: build({ status: 'UNKNOWN' }) }).label).toBe('Unknown')
  })
})

describe('anyBuildActive', () => {
  const row = (b: CiBuildStatus | null): CiBuildTypeStatus => ({
    buildTypeId: 'X',
    label: 'X',
    build: b,
  })

  it('is true while any build is queued or running', () => {
    expect(anyBuildActive([row(null), row(build({ state: 'queued', status: 'UNKNOWN' }))])).toBe(
      true,
    )
    expect(anyBuildActive([row(build({ state: 'running' }))])).toBe(true)
  })

  it('is false for finished or absent builds', () => {
    expect(anyBuildActive([row(null), row(build({ status: 'FAILURE' }))])).toBe(false)
    expect(anyBuildActive([])).toBe(false)
  })
})
