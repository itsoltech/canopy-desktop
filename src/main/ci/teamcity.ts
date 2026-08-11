import { ResultAsync, errAsync, okAsync } from 'neverthrow'
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
import { ciErrorMessage, type CiError } from './errors'
import { parsePromptParameters } from './parameters'
import { parseActivity, parseBranches, parseTcDate, type RawActivityResponse } from './activity'
import { withCiDegradedCauses } from './degraded'

// TeamCity REST client (https://www.jetbrains.com/help/teamcity/rest/). Pure
// response-mapping helpers are exported for unit tests; network access follows the
// task-tracker provider conventions: Bearer token, 15s timeout, redirects refused
// (baseUrl comes from repo config — a redirect would forward the token elsewhere).

const BUILD_FIELDS =
  'id,number,state,status,statusText,percentageComplete,webUrl,branchName,queuedDate,startDate,finishDate'
const TEAMCITY_LOCATOR_STRUCTURAL_CHARS = /[(),:]/u
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024

/** Refs are parenthesized in TeamCity locators; structural characters must not reach the sink. */
export function isTeamCityLocatorSafeRef(ref: string): boolean {
  return ref.length > 0 && ref.length <= 255 && !TEAMCITY_LOCATOR_STRUCTURAL_CHARS.test(ref)
}

interface RawBuild {
  id: number
  number?: string
  state?: string
  status?: string
  statusText?: string
  percentageComplete?: number
  webUrl?: string
  branchName?: string
  queuedDate?: string
  startDate?: string
  finishDate?: string
  buildType?: { id?: string }
}

export interface CiBuildWithType extends CiBuildStatus {
  buildTypeId: string
}

/**
 * Locator for "the newest build of this configuration on this branch, queued and
 * running included". `defaultFilter:false` keeps cancelled/failed-to-start builds
 * visible — hiding them would report a stale older build as current. Values are
 * parenthesized: branch names routinely contain `/` (dimension separators
 * otherwise). Characters that would escape the parentheses are rejected at the
 * live TeamCity status paths before this helper is called.
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
    statusText: raw.statusText,
    percentageComplete: raw.percentageComplete,
    webUrl: raw.webUrl ?? '',
    branchName: raw.branchName,
    queuedAt: parseTcDate(raw.queuedDate),
    startedAt: parseTcDate(raw.startDate),
    finishedAt: parseTcDate(raw.finishDate),
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

function redactTeamCityToken(message: string, token?: string): string {
  // These messages can become plaintext credential verification metadata, so redact before the
  // response body is truncated, persisted, or returned to the renderer.
  return token ? message.replaceAll(token, '[redacted]') : message
}

const apiError = (status: number, message: string, token?: string): CiError => ({
  _tag: 'CiApiError',
  status,
  message: redactTeamCityToken(message, token),
})

async function readBoundedResponse(response: Response): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array()
  const reader = response.body.getReader()
  const chunks: Uint8Array[] = []
  let length = 0
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      length += value.byteLength
      if (length > MAX_RESPONSE_BYTES) {
        await reader.cancel().catch(() => undefined)
        throw new Error('TeamCity response exceeds the size limit')
      }
      chunks.push(value)
    }
  } finally {
    reader.releaseLock()
  }
  const bytes = new Uint8Array(length)
  let offset = 0
  for (const chunk of chunks) {
    bytes.set(chunk, offset)
    offset += chunk.byteLength
  }
  return bytes
}

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
    (e) => apiError(0, errorMessage(e), token),
  ).andThen((res) => {
    const contentLength = Number(res.headers.get('content-length'))
    if (Number.isFinite(contentLength) && contentLength > MAX_RESPONSE_BYTES) {
      return errAsync(apiError(res.ok ? 0 : res.status, 'TeamCity response exceeds the size limit'))
    }

    return fromExternalCall(readBoundedResponse(res), (e) =>
      apiError(res.ok ? 0 : res.status, errorMessage(e), token),
    ).andThen((bytes) => {
      const body = Buffer.from(bytes).toString('utf8')
      if (!res.ok) {
        return errAsync(
          apiError(
            res.status,
            redactTeamCityToken(body.trim(), token).slice(0, 300) || res.statusText,
          ),
        )
      }

      try {
        return okAsync((body ? JSON.parse(body) : undefined) as T)
      } catch {
        return errAsync(apiError(0, 'TeamCity returned malformed JSON'))
      }
    })
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
): ResultAsync<CiBuildWithType, CiError> {
  return tcFetch<RawBuild>(
    baseUrl,
    token,
    `/app/rest/builds/id:${buildId}?fields=${BUILD_FIELDS},buildType(id)`,
  ).andThen((raw) =>
    raw.buildType?.id
      ? okAsync({ ...mapBuild(raw), buildTypeId: raw.buildType.id })
      : errAsync(apiError(0, 'TeamCity returned a build without its build configuration')),
  )
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
  // BuildQueueLocator does not support the `branch` or `defaultFilter` dimensions used by the
  // BuildLocator siblings below. Adding either makes the queue slice fail and activity partial.
  return `${activityBuildTypesLocator(buildTypeIds)},count:20`
}

/**
 * Activity for the repository's configured build types only.
 *
 * `branch` narrows the SERVER-side slice. That is the whole point of the history
 * window's filter: `count:10` is applied by TeamCity before anything reaches us, so
 * filtering the response instead would hand back an empty list for any branch whose
 * builds are older than the ten newest in the repository.
 */
export function fetchActivity(
  baseUrl: string,
  token: string,
  buildTypeIds: string[],
  branch?: string,
): ResultAsync<CiActivity, CiError> {
  if (branch !== undefined && !isTeamCityLocatorSafeRef(branch)) {
    return errAsync<CiActivity, CiError>({
      _tag: 'CiApiError',
      status: 0,
      message: 'TeamCity branch contains locator-unsafe characters',
    })
  }
  const fields =
    'count,build(id,number,status,statusText,percentageComplete,webUrl,branchName,queuedDate,startDate,finishDate,buildType(id,name))'
  const scope = activityBuildTypesLocator(buildTypeIds)
  const branchLocator = branch === undefined ? 'branch:(default:any)' : `branch:(name:(${branch}))`
  interface ActivityPart {
    response: RawActivityResponse
    error?: string
    cause?: CiError
  }
  const collect = (label: string, path: string): ResultAsync<ActivityPart, CiError> =>
    tcFetch<RawActivityResponse>(baseUrl, token, path)
      .map((response) => ({ response }))
      .orElse((error) =>
        okAsync<ActivityPart, CiError>({
          response: {},
          error: `${label}: ${ciErrorMessage(error)}`,
          cause: error,
        }),
      )
  return ResultAsync.combine([
    collect(
      'Running builds',
      `/app/rest/builds?locator=${encodeURIComponent(`${scope},running:true,${branchLocator},defaultFilter:false,count:20`)}&fields=${fields}`,
    ),
    collect(
      'Queued builds',
      `/app/rest/buildQueue?locator=${encodeURIComponent(queuedActivityLocator(buildTypeIds))}&fields=${fields}`,
    ),
    collect(
      'Recent builds',
      `/app/rest/builds?locator=${encodeURIComponent(`${scope},state:finished,${branchLocator},defaultFilter:false,count:10`)}&fields=${fields}`,
    ),
  ]).andThen(([running, queued, recent]) => {
    const partialErrors = [running.error, queued.error, recent.error].filter(
      (error): error is string => Boolean(error),
    )
    const causes = [running.cause, queued.cause, recent.cause].filter((cause): cause is CiError =>
      Boolean(cause),
    )
    if (partialErrors.length === 3) {
      const authFailure = causes.find(
        (cause) => cause._tag === 'CiApiError' && (cause.status === 401 || cause.status === 403),
      )
      // One reason, not three. All three slices hit the same server with the same token, so
      // a rejected token produced three identical sentences; joining them under a wrapper
      // that states the status again put the same line in front of the user four times.
      // Returning the shared cause itself keeps the type honest and says it once — the
      // slice names only carry information when the slices failed differently.
      const first = causes[0]
      if (causes.length === 3 && first && new Set(causes.map(ciErrorMessage)).size === 1) {
        return errAsync<CiActivity, CiError>(first)
      }
      return errAsync<CiActivity, CiError>({
        _tag: 'CiApiError',
        status: authFailure?._tag === 'CiApiError' ? authFailure.status : 0,
        message: partialErrors.join(' · '),
      })
    }
    const activity = parseActivity(running.response, queued.response, recent.response)
    const result: CiActivity = {
      ...activity,
      // The queue is the one slice that cannot be narrowed server-side (see
      // `queuedActivityLocator`), so it is filtered here instead. Safe in a way the
      // other two are not: `count:20` covers the whole queue in practice, so nothing
      // is hidden behind the cap the way a finished build would be.
      ...(branch === undefined
        ? {}
        : { queued: activity.queued.filter((build) => build.branchName === branch) }),
      ...(partialErrors.length ? { partialErrors } : {}),
    }
    return okAsync<CiActivity, CiError>(
      partialErrors.length ? withCiDegradedCauses(result, causes) : result,
    )
  })
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
