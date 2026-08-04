import { ResultAsync, errAsync } from 'neverthrow'
import { fromExternalCall, errorMessage } from '../errors'
import type {
  CiActivity,
  CiBuildResult,
  CiBuildState,
  CiBuildStatus,
  CiParameter,
  CiServerBuildType,
  CiTriggerResult,
} from './types'
import type { CiError } from './errors'
import { parsePromptParameters } from './parameters'
import { parseActivity, parseBranches, type RawActivityResponse } from './activity'

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
    raw.status === 'SUCCESS' || raw.status === 'FAILURE' || raw.status === 'ERROR'
      ? raw.status
      : 'UNKNOWN'
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

/** Queue a build of the configuration on the given branch, optionally with custom parameters. */
export function triggerBuild(
  baseUrl: string,
  token: string,
  buildTypeId: string,
  branch: string,
  properties?: Array<{ name: string; value: string }>,
): ResultAsync<CiTriggerResult, CiError> {
  return tcFetch<{ id: number; webUrl?: string; branchName?: string }>(
    baseUrl,
    token,
    '/app/rest/buildQueue',
    {
      method: 'POST',
      body: JSON.stringify({
        buildType: { id: buildTypeId },
        branchName: branch,
        comment: { text: 'Triggered from Canopy' },
        ...(properties?.length ? { properties: { property: properties } } : {}),
      }),
    },
  ).map((res) => ({ buildId: res.id, webUrl: res.webUrl ?? '', branchName: res.branchName }))
}

/** All build configurations on the server — source for the per-repo config picker. */
export function fetchBuildTypes(
  baseUrl: string,
  token: string,
): ResultAsync<CiServerBuildType[], CiError> {
  return tcFetch<{ buildType?: Array<{ id?: string; name?: string; projectName?: string }> }>(
    baseUrl,
    token,
    '/app/rest/buildTypes?fields=buildType(id,name,projectName)',
  ).map((res) =>
    (res.buildType ?? []).flatMap((bt) =>
      bt.id ? [{ id: bt.id, name: bt.name ?? bt.id, projectName: bt.projectName ?? '' }] : [],
    ),
  )
}

/** A BuildTypeLocator union used to scope every activity query to configured jobs. */
export function activityBuildTypesLocator(buildTypeIds: string[]): string {
  return `buildType:(${buildTypeIds.map((id) => `item:(id:${id})`).join(',')})`
}

export function queuedActivityLocator(buildTypeIds: string[]): string {
  return `${activityBuildTypesLocator(buildTypeIds)},branch:(default:any),defaultFilter:false,count:20`
}

/** Activity for the repository's configured build types only. */
export function fetchActivity(
  baseUrl: string,
  token: string,
  buildTypeIds: string[],
): ResultAsync<CiActivity, CiError> {
  const fields =
    'count,build(id,number,status,statusText,percentageComplete,webUrl,branchName,queuedDate,startDate,finishDate,buildType(id,name))'
  const scope = activityBuildTypesLocator(buildTypeIds)
  return ResultAsync.combine([
    tcFetch<RawActivityResponse>(
      baseUrl,
      token,
      `/app/rest/builds?locator=${encodeURIComponent(`${scope},running:true,branch:(default:any),defaultFilter:false,count:20`)}&fields=${fields}`,
    ),
    tcFetch<RawActivityResponse>(
      baseUrl,
      token,
      `/app/rest/buildQueue?locator=${encodeURIComponent(queuedActivityLocator(buildTypeIds))}&fields=${fields}`,
    ),
    tcFetch<RawActivityResponse>(
      baseUrl,
      token,
      `/app/rest/builds?locator=${encodeURIComponent(`${scope},state:finished,branch:(default:any),defaultFilter:false,count:10`)}&fields=${fields}`,
    ),
  ]).map(([running, queued, recent]) => parseActivity(running, queued, recent))
}

/** Branches TeamCity knows for a build configuration — default branch first. */
export function fetchBranches(
  baseUrl: string,
  token: string,
  buildTypeId: string,
): ResultAsync<string[], CiError> {
  return tcFetch<{ count?: number; branch?: Array<{ name?: string; default?: boolean }> }>(
    baseUrl,
    token,
    `/app/rest/buildTypes/id:${buildTypeId}/branches?locator=policy:VCS_BRANCHES&fields=branch(name,default)`,
  ).map(parseBranches)
}

/** The parameters TeamCity would prompt for in its "Run custom build" dialog. */
export function fetchPromptParameters(
  baseUrl: string,
  token: string,
  buildTypeId: string,
): ResultAsync<CiParameter[], CiError> {
  return tcFetch<{
    count?: number
    property?: Array<{ name: string; value?: string; type?: { rawValue?: string } }>
  }>(
    baseUrl,
    token,
    `/app/rest/buildTypes/id:${buildTypeId}/parameters?fields=property(name,value,type(rawValue))`,
  ).map(parsePromptParameters)
}
