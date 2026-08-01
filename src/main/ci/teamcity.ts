import { errAsync, type ResultAsync } from 'neverthrow'
import { fromExternalCall, errorMessage } from '../errors'
import type { CiBuildResult, CiBuildState, CiBuildStatus, CiError, CiTriggerResult } from './types'

// TeamCity REST client (https://www.jetbrains.com/help/teamcity/rest/). Pure
// response-mapping helpers are exported for unit tests; network access follows the
// task-tracker provider conventions: Bearer token, 15s timeout, redirects refused
// (baseUrl comes from repo config — a redirect would forward the token elsewhere).

const BUILD_FIELDS = 'id,number,state,status,percentageComplete,webUrl,branchName'

interface RawBuild {
  id: number
  number?: string
  state?: string
  status?: string
  percentageComplete?: number
  webUrl?: string
  branchName?: string
}

/**
 * Locator for "the newest build of this configuration on this branch, queued and
 * running included". `defaultFilter:false` keeps cancelled/failed-to-start builds
 * visible — hiding them would report a stale older build as current. Values are
 * parenthesized: branch names routinely contain `/` (dimension separators
 * otherwise). Characters that would escape the parentheses are rejected at the
 * IPC boundary.
 */
export function buildBranchLocator(buildTypeId: string, branch: string): string {
  return `buildType:(id:${buildTypeId}),branch:(name:(${branch})),running:any,defaultFilter:false,count:1`
}

export function mapBuild(raw: RawBuild): CiBuildStatus {
  const state: CiBuildState =
    raw.state === 'queued' || raw.state === 'running' ? raw.state : 'finished'
  const status: CiBuildResult =
    raw.status === 'SUCCESS' || raw.status === 'FAILURE' ? raw.status : 'UNKNOWN'
  return {
    id: raw.id,
    number: raw.number ?? String(raw.id),
    state,
    status,
    percentageComplete: raw.percentageComplete,
    webUrl: raw.webUrl ?? '',
    branchName: raw.branchName,
  }
}

export function parseBuildsResponse(json: {
  count?: number
  build?: RawBuild[]
}): CiBuildStatus | null {
  const first = json.build?.[0]
  return first ? mapBuild(first) : null
}

// --- Network layer ---

const apiError = (status: number, message: string): CiError => ({
  _tag: 'CiApiError',
  status,
  message,
})

function tcFetch<T>(
  baseUrl: string,
  token: string,
  path: string,
  init?: { method?: string; body?: string },
): ResultAsync<T, CiError> {
  const url = `${baseUrl.replace(/\/$/, '')}${path}`
  return fromExternalCall(
    fetch(url, {
      method: init?.method ?? 'GET',
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: 'application/json',
        ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
      },
      body: init?.body,
      redirect: 'error',
      signal: AbortSignal.timeout(15_000),
    }),
    (e) => apiError(0, errorMessage(e)),
  ).andThen((res) => {
    if (!res.ok) {
      return fromExternalCall(
        res.text().catch(() => ''),
        (e) => apiError(res.status, errorMessage(e)),
      ).andThen((body) =>
        errAsync(apiError(res.status, body.trim().slice(0, 300) || res.statusText)),
      )
    }
    return fromExternalCall(res.json() as Promise<T>, (e) => apiError(0, errorMessage(e)))
  })
}

/** Cheap authenticated probe used by the Settings connection test. */
export function testConnection(baseUrl: string, token: string): ResultAsync<void, CiError> {
  return tcFetch<{ version?: string }>(baseUrl, token, '/app/rest/server').map(() => undefined)
}

/** Newest build (queued/running included) of a configuration on a branch, or null. */
export function fetchBuildForBranch(
  baseUrl: string,
  token: string,
  buildTypeId: string,
  branch: string,
): ResultAsync<CiBuildStatus | null, CiError> {
  const locator = encodeURIComponent(buildBranchLocator(buildTypeId, branch))
  return tcFetch<{ count?: number; build?: RawBuild[] }>(
    baseUrl,
    token,
    `/app/rest/builds?locator=${locator}&fields=count,build(${BUILD_FIELDS})`,
  ).map(parseBuildsResponse)
}

/** Single build by id — used to watch a build triggered from Canopy to completion. */
export function fetchBuild(
  baseUrl: string,
  token: string,
  buildId: number,
): ResultAsync<CiBuildStatus, CiError> {
  return tcFetch<RawBuild>(
    baseUrl,
    token,
    `/app/rest/builds/id:${buildId}?fields=${BUILD_FIELDS}`,
  ).map(mapBuild)
}

/** Queue a build of the configuration on the given branch. */
export function triggerBuild(
  baseUrl: string,
  token: string,
  buildTypeId: string,
  branch: string,
): ResultAsync<CiTriggerResult, CiError> {
  return tcFetch<{ id: number; webUrl?: string }>(baseUrl, token, '/app/rest/buildQueue', {
    method: 'POST',
    body: JSON.stringify({
      buildType: { id: buildTypeId },
      branchName: branch,
      comment: { text: 'Triggered from Canopy' },
    }),
  }).map((res) => ({ buildId: res.id, webUrl: res.webUrl ?? '' }))
}
