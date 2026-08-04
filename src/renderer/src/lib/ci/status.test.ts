import { describe, expect, it } from 'vitest'
import {
  anyBuildActive,
  ciChip,
  ciRunChip,
  ciRunStatusTextClass,
  ciStatusTextClass,
} from './status'

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
    // ERROR is a red state in TeamCity's own UI — it must read as a failure,
    // not share the neutral chip with "never built".
    expect(ciChip({ build: build({ status: 'ERROR' }) }).label).toBe('Failed')
    expect(ciChip({ build: build({ status: 'UNKNOWN' }) }).label).toBe('Unknown')
  })

  it('renders Unknown for status tokens the app does not model', () => {
    // Activity rows feed TeamCity's raw status through — ERROR, CANCELLED and
    // friends must not surface as bare API tokens.
    expect(ciChip({ build: { ...build({}), status: 'CANCELLED' } }).label).toBe('Unknown')
    expect(ciChip({ build: { ...build({}), status: undefined } }).label).toBe('Unknown')
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

describe('ciStatusTextClass', () => {
  it('uses outcome colours only after a build finishes', () => {
    expect(ciStatusTextClass(build({ state: 'running', status: 'SUCCESS' }))).toBe(
      'text-text-muted',
    )
    expect(ciStatusTextClass(build({ status: 'SUCCESS' }))).toBe('text-success-text')
    expect(ciStatusTextClass(build({ status: 'FAILURE' }))).toBe('text-danger-text')
    expect(ciStatusTextClass(build({ status: 'ERROR' }))).toBe('text-danger-text')
  })
})

describe('ciRunChip', () => {
  const run = (overrides: Partial<CiRun> = {}): CiRun => ({
    provider: 'github-actions',
    runId: '1',
    number: '1',
    jobId: '.github/workflows/check.yml',
    jobLabel: 'Check',
    state: 'finished',
    conclusion: 'success',
    statusText: undefined,
    webUrl: 'https://github.com/run/1',
    ref: { name: 'next', kind: 'branch' },
    queuedAt: undefined,
    startedAt: undefined,
    finishedAt: undefined,
    ...overrides,
  })

  it('uses one vocabulary for GitHub run cards and history', () => {
    expect(ciRunChip({ run: null }).label).toBe('No runs')
    expect(ciRunChip({ run: null, error: 'offline' }).label).toBe('Unavailable')
    expect(ciRunChip({ run: run({ state: 'waiting' }) }).label).toBe('Waiting')
    expect(ciRunChip({ run: run({ state: 'running' }) }).label).toBe('Running')
    expect(ciRunChip({ run: run({ conclusion: 'failure' }) }).label).toBe('Failed')
    expect(ciRunChip({ run: run({ conclusion: 'cancelled' }) }).label).toBe('Cancelled')
    expect(ciRunChip({ run: run({ state: 'unknown', conclusion: 'success' }) }).label).toBe(
      'Unknown',
    )
    expect(ciRunChip({ run: run({ state: 'unknown', conclusion: 'failure' }) }).label).toBe(
      'Unknown',
    )
  })

  it('uses outcome colours only after a run finishes', () => {
    expect(ciRunStatusTextClass(run({ state: 'running', conclusion: 'success' }))).toBe(
      'text-text-muted',
    )
    expect(ciRunStatusTextClass(run({ conclusion: 'success' }))).toBe('text-success-text')
    expect(ciRunStatusTextClass(run({ conclusion: 'failure' }))).toBe('text-danger-text')
  })
})
