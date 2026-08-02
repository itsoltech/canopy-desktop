import { match } from 'ts-pattern'
import type { CiBuildTypeStatus } from './types'

export interface CiChip {
  label: string
  cls: string
}

/** Status chip for a build-type row — colors follow the PR state chip conventions. */
export function ciChip(row: Pick<CiBuildTypeStatus, 'build' | 'error'>): CiChip {
  // A failed fetch is NOT "no builds": `build` is null in both cases, so the error
  // has to win here or an outage reads as "this branch was never built".
  if (row.error) return { label: 'Unavailable', cls: 'bg-warning-bg text-warning-text' }
  const build = row.build
  if (!build) return { label: 'No builds', cls: 'bg-active text-text-muted' }
  return match(build)
    .with({ state: 'queued' }, () => ({ label: 'Queued', cls: 'bg-active text-text-muted' }))
    .with({ state: 'running' }, (b) => ({
      label: b.percentageComplete != null ? `Running ${b.percentageComplete}%` : 'Running',
      cls: 'bg-accent-bg text-accent-text',
    }))
    .with({ status: 'SUCCESS' }, () => ({
      label: 'Success',
      cls: 'bg-success-bg text-success-text',
    }))
    .with({ status: 'FAILURE' }, () => ({ label: 'Failed', cls: 'bg-danger-bg text-danger-text' }))
    .otherwise(() => ({ label: 'Unknown', cls: 'bg-active text-text-muted' }))
}

/** Drives the faster poll interval — a queued or running build changes state soon. */
export function anyBuildActive(rows: CiBuildTypeStatus[]): boolean {
  return rows.some((r) => r.build != null && r.build.state !== 'finished')
}
