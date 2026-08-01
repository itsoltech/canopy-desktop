// Parsing for the server-wide activity view (what is running / queued on TeamCity
// right now) and for a build configuration's branch list.

import type { CiActivity, CiActivityBuild } from './types'

interface RawActivityBuild {
  id: number
  number?: string
  // Present in responses but unused: the endpoint's locator already fixes the state.
  state?: string
  status?: string
  percentageComplete?: number
  webUrl?: string
  branchName?: string
  buildType?: { id?: string; name?: string }
}

export interface RawActivityResponse {
  count?: number
  build?: RawActivityBuild[]
}

function mapActivityBuild(raw: RawActivityBuild, state: 'running' | 'queued'): CiActivityBuild {
  return {
    id: raw.id,
    number: raw.number,
    state,
    percentageComplete: raw.percentageComplete,
    webUrl: raw.webUrl ?? '',
    branchName: raw.branchName,
    buildTypeId: raw.buildType?.id ?? '',
    buildTypeName: raw.buildType?.name || (raw.buildType?.id ?? ''),
  }
}

export function parseActivity(
  runningJson: RawActivityResponse,
  queuedJson: RawActivityResponse,
): CiActivity {
  return {
    running: (runningJson.build ?? []).map((b) => mapActivityBuild(b, 'running')),
    queued: (queuedJson.build ?? []).map((b) => mapActivityBuild(b, 'queued')),
  }
}

/** Branch names of a build configuration, TC's default branch first. */
export function parseBranches(json: {
  count?: number
  branch?: Array<{ name?: string; default?: boolean }>
}): string[] {
  const entries = (json.branch ?? []).filter((b) => !!b.name)
  return [
    ...entries.filter((b) => b.default).map((b) => b.name as string),
    ...entries.filter((b) => !b.default).map((b) => b.name as string),
  ]
}
