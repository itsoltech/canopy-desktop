// Parsing for the server-wide activity view (what is running / queued on TeamCity
// right now) and for a build configuration's branch list.

import type { CiActivity, CiActivityBuild } from './types'

interface RawActivityBuild {
  id: number
  number?: string
  // Present in responses but unused: each endpoint's locator already fixes the state.
  state?: string
  status?: string
  percentageComplete?: number
  webUrl?: string
  branchName?: string
  queuedDate?: string
  startDate?: string
  finishDate?: string
  buildType?: { id?: string; name?: string }
}

/** TeamCity timestamp (`20260801T172347+0200`) → epoch ms, undefined when absent/malformed. */
export function parseTcDate(raw: string | undefined): number | undefined {
  if (!raw) return undefined
  const m = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})([+-]\d{4}|Z)?$/.exec(raw)
  if (!m) return undefined
  const zone = m[7] ? (m[7] === 'Z' ? 'Z' : `${m[7].slice(0, 3)}:${m[7].slice(3)}`) : ''
  const ts = Date.parse(`${m[1]}-${m[2]}-${m[3]}T${m[4]}:${m[5]}:${m[6]}${zone}`)
  return Number.isNaN(ts) ? undefined : ts
}

export interface RawActivityResponse {
  count?: number
  build?: RawActivityBuild[]
}

function mapActivityBuild(
  raw: RawActivityBuild,
  state: 'running' | 'queued' | 'finished',
): CiActivityBuild {
  return {
    id: raw.id,
    number: raw.number,
    state,
    status: raw.status,
    percentageComplete: raw.percentageComplete,
    webUrl: raw.webUrl ?? '',
    branchName: raw.branchName,
    queuedAt: parseTcDate(raw.queuedDate),
    startedAt: parseTcDate(raw.startDate),
    finishedAt: parseTcDate(raw.finishDate),
    buildTypeId: raw.buildType?.id ?? '',
    buildTypeName: raw.buildType?.name || (raw.buildType?.id ?? ''),
  }
}

export function parseActivity(
  runningJson: RawActivityResponse,
  queuedJson: RawActivityResponse,
  recentJson: RawActivityResponse,
): CiActivity {
  return {
    running: (runningJson.build ?? []).map((b) => mapActivityBuild(b, 'running')),
    queued: (queuedJson.build ?? []).map((b) => mapActivityBuild(b, 'queued')),
    recent: (recentJson.build ?? []).map((b) => mapActivityBuild(b, 'finished')),
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
