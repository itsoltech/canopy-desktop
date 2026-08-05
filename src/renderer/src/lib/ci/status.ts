import { match, P } from 'ts-pattern'
import type { CiBuildTypeStatus, CiRun } from './types'

export interface CiChip {
  label: string
  cls: string
}

interface TimestampedRun {
  queuedAt: number | undefined
  startedAt: number | undefined
  finishedAt: number | undefined
}

export interface CiLastRunTimestamp {
  value: number
  label: 'Queued' | 'Started' | 'Finished'
}

/** Timestamp plus wording that cannot contradict the run's current state. */
export function ciLastRunTimestampInfo(run: TimestampedRun): CiLastRunTimestamp | undefined {
  if (run.finishedAt != null) return { value: run.finishedAt, label: 'Finished' }
  if (run.startedAt != null) return { value: run.startedAt, label: 'Started' }
  if (run.queuedAt != null) return { value: run.queuedAt, label: 'Queued' }
  return undefined
}

/** The fields the chip logic reads — CiBuildStatus and CiActivityBuild both fit. */
interface ChipBuild {
  state: 'queued' | 'running' | 'finished'
  status: string | undefined
  percentageComplete: number | undefined
}

/**
 * Colour for TeamCity's build-specific status text. Only a finished build has an
 * outcome: TeamCity may report SUCCESS while a running build merely has not failed yet.
 */
export function ciStatusTextClass(build: ChipBuild): string {
  return match(build)
    .with({ state: 'finished', status: 'SUCCESS' }, () => 'text-success-text')
    .with({ state: 'finished', status: P.union('FAILURE', 'ERROR') }, () => 'text-danger-text')
    .otherwise(() => 'text-text-muted')
}

/** Colour for a GitHub Actions run's status line — the run counterpart above. */
export function ciRunStatusTextClass(run: Pick<CiRun, 'state' | 'conclusion'>): string {
  return match(run)
    .with({ state: 'finished', conclusion: 'success' }, () => 'text-success-text')
    .with({ state: 'finished', conclusion: 'failure' }, () => 'text-danger-text')
    .otherwise(() => 'text-text-muted')
}

/**
 * Status chip for a build row — the single owner of the chip vocabulary for both the
 * Last-job card and the Jobs history window, so the two surfaces can never disagree
 * on the label or color for the same build state.
 */
export function ciChip(row: { build: ChipBuild | null; error?: string }): CiChip {
  // A failed fetch is NOT "no builds": `build` is null in both cases, so the error
  // has to win here or an outage reads as "this branch was never built".
  if (row.error) return { label: 'Unavailable', cls: 'bg-warning-bg text-warning-text' }
  const build = row.build
  if (!build) return { label: 'No builds', cls: 'bg-active text-text-muted' }
  return (
    match(build)
      .with({ state: 'queued' }, () => ({ label: 'Queued', cls: 'bg-active text-text-muted' }))
      .with({ state: 'running' }, (b) => ({
        label: b.percentageComplete != null ? `Running ${b.percentageComplete}%` : 'Running',
        cls: 'bg-accent-bg text-accent-text',
      }))
      .with({ status: 'SUCCESS' }, () => ({
        label: 'Success',
        cls: 'bg-success-bg text-success-text',
      }))
      // ERROR is TeamCity's infra/agent failure — red in its own UI, so it must not
      // share the neutral Unknown chip with "never built" and cancelled builds.
      .with({ status: P.union('FAILURE', 'ERROR') }, () => ({
        label: 'Failed',
        cls: 'bg-danger-bg text-danger-text',
      }))
      .otherwise(() => ({ label: 'Unknown', cls: 'bg-active text-text-muted' }))
  )
}

/** Provider-neutral status chip for GitHub Actions run surfaces. */
export function ciRunChip(row: { run: CiRun | null; error?: string }): CiChip {
  if (row.error) return { label: 'Unavailable', cls: 'bg-warning-bg text-warning-text' }
  const run = row.run
  if (!run) return { label: 'No runs', cls: 'bg-active text-text-muted' }
  return match(run)
    .with({ state: 'waiting' }, () => ({
      label: 'Waiting',
      cls: 'bg-warning-bg text-warning-text',
    }))
    .with({ state: 'running' }, () => ({
      label: 'Running',
      cls: 'bg-accent-bg text-accent-text',
    }))
    .with({ state: 'queued' }, () => ({ label: 'Queued', cls: 'bg-active text-text-muted' }))
    .with({ state: 'unknown' }, () => ({ label: 'Unknown', cls: 'bg-active text-text-muted' }))
    .with({ conclusion: 'success' }, () => ({
      label: 'Success',
      cls: 'bg-success-bg text-success-text',
    }))
    .with({ conclusion: 'failure' }, () => ({
      label: 'Failed',
      cls: 'bg-danger-bg text-danger-text',
    }))
    .with({ conclusion: 'cancelled' }, () => ({
      label: 'Cancelled',
      cls: 'bg-active text-text-muted',
    }))
    .with({ conclusion: 'neutral' }, () => ({
      label: 'Neutral',
      cls: 'bg-active text-text-muted',
    }))
    .otherwise(() => ({ label: 'Unknown', cls: 'bg-active text-text-muted' }))
}

/** Drives the faster poll interval — a queued or running build changes state soon. */
export function anyBuildActive(rows: CiBuildTypeStatus[]): boolean {
  return rows.some((r) => r.build != null && r.build.state !== 'finished')
}
