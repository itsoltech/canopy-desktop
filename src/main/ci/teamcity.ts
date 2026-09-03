import { lookup } from 'dns'
import type { LookupAddress } from 'dns'
import { isIP } from 'net'
import type { LookupFunction } from 'net'
import { Agent } from 'undici'
import { ResultAsync, errAsync, okAsync } from 'neverthrow'
import { fromExternalCall, errorMessage } from '../errors'
import { isPrivateIp } from '../security/validateUrl'
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
import type { TeamCityConnectionPolicy } from './teamcityAccess'

// TeamCity REST client (https://www.jetbrains.com/help/teamcity/rest/). Pure
// response-mapping helpers are exported for unit tests; network access follows the
// task-tracker provider conventions: Bearer token, 15s timeout, redirects refused
// (baseUrl comes from repo config — a redirect would forward the token elsewhere).

const BUILD_FIELDS =
  'id,number,state,status,statusText,percentageComplete,webUrl,branchName,queuedDate,startDate,finishDate'
const TEAMCITY_LOCATOR_STRUCTURAL_CHARS = /[(),:]/u
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024

type JsonRecord = Record<string, unknown>
type RuntimeGuard = (value: unknown) => boolean

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function hasOptionalString(record: JsonRecord, key: string): boolean {
  return record[key] === undefined || typeof record[key] === 'string'
}

function hasOptionalBoolean(record: JsonRecord, key: string): boolean {
  return record[key] === undefined || typeof record[key] === 'boolean'
}

function hasOptionalNumber(record: JsonRecord, key: string): boolean {
  return record[key] === undefined || typeof record[key] === 'number'
}

function isCollection(value: unknown, key: string, itemGuard: RuntimeGuard): boolean {
  if (!isRecord(value)) return false
  const items = value[key]
  return items === undefined || (Array.isArray(items) && items.every(itemGuard))
}

function isRawBuild(value: unknown): value is RawBuild {
  if (!isRecord(value) || typeof value.id !== 'number') return false
  if (
    !hasOptionalString(value, 'number') ||
    !hasOptionalString(value, 'state') ||
    !hasOptionalString(value, 'status') ||
    !hasOptionalString(value, 'statusText') ||
    !hasOptionalNumber(value, 'percentageComplete') ||
    !hasOptionalString(value, 'webUrl') ||
    !hasOptionalString(value, 'branchName') ||
    !hasOptionalString(value, 'queuedDate') ||
    !hasOptionalString(value, 'startDate') ||
    !hasOptionalString(value, 'finishDate')
  ) {
    return false
  }
  return (
    value.buildType === undefined ||
    (isRecord(value.buildType) && hasOptionalString(value.buildType, 'id'))
  )
}

function isBuildCollection(value: unknown): boolean {
  return isCollection(value, 'build', isRawBuild)
}

function isBuildType(value: unknown): boolean {
  return (
    isRecord(value) &&
    hasOptionalString(value, 'id') &&
    hasOptionalString(value, 'name') &&
    hasOptionalString(value, 'projectName')
  )
}

function isBranch(value: unknown): boolean {
  return isRecord(value) && hasOptionalString(value, 'name') && hasOptionalBoolean(value, 'default')
}

function isParameter(value: unknown): boolean {
  if (!isRecord(value) || typeof value.name !== 'string' || !hasOptionalString(value, 'value')) {
    return false
  }
  return (
    value.type === undefined || (isRecord(value.type) && hasOptionalString(value.type, 'rawValue'))
  )
}

function decodeTcResponse<T>(value: unknown, guard: RuntimeGuard): ResultAsync<T, CiError> {
  return guard(value)
    ? okAsync(value as T)
    : errAsync(apiError(0, 'TeamCity returned an invalid response shape'))
}

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

class TeamCityPrivateOriginBlocked extends Error {}

/** Validates the whole DNS set without discarding Undici's multi-address fallback. */
export function validateTeamCityConnectionAddresses(
  addresses: readonly LookupAddress[],
  policy: TeamCityConnectionPolicy,
): LookupAddress[] {
  if (addresses.length === 0) throw new Error('Could not resolve TeamCity server')
  if (!policy.allowPrivate && addresses.some((address) => isPrivateIp(address.address))) {
    throw new TeamCityPrivateOriginBlocked('TeamCity resolved to a private network address')
  }
  return [...addresses]
}

export function selectTeamCityConnectionAddress(
  addresses: readonly LookupAddress[],
  policy: TeamCityConnectionPolicy,
): LookupAddress {
  return validateTeamCityConnectionAddresses(addresses, policy)[0]
}

function guardedLookup(policy: TeamCityConnectionPolicy): LookupFunction {
  return (hostname, options, callback) => {
    lookup(
      hostname,
      {
        family: options.family,
        hints: options.hints,
        all: true,
        verbatim: true,
      },
      (error, addresses) => {
        if (error) {
          callback(error, '')
          return
        }
        try {
          const approved = validateTeamCityConnectionAddresses(addresses, policy)
          if (options.all) callback(null, approved)
          else callback(null, approved[0].address, approved[0].family)
        } catch (cause) {
          callback(cause instanceof Error ? cause : new Error('Unsafe TeamCity origin'), '')
        }
      },
    )
  }
}

const teamCityDispatchers = new Map<boolean, Agent>()

function dispatcherFor(policy: TeamCityConnectionPolicy): Agent {
  let dispatcher = teamCityDispatchers.get(policy.allowPrivate)
  if (!dispatcher) {
    dispatcher = new Agent({ connect: { lookup: guardedLookup(policy) } })
    teamCityDispatchers.set(policy.allowPrivate, dispatcher)
  }
  return dispatcher
}

function isPrivateOriginBlocked(error: unknown): boolean {
  if (error instanceof TeamCityPrivateOriginBlocked) return true
  if (!error || typeof error !== 'object' || !('cause' in error)) return false
  return error.cause instanceof TeamCityPrivateOriginBlocked
}

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

const ambiguousDispatchError = (detailsUrl: string): CiError => ({
  _tag: 'CiDispatchAmbiguous',
  provider: 'teamcity',
  detailsUrl,
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
  connection: TeamCityConnectionPolicy,
  path: string,
  init?: { method?: string; body?: string; ambiguousDispatchUrl?: string },
): ResultAsync<T, CiError> {
  const url = `${baseUrl.replace(/\/$/, '')}${path}`
  const hostname = new URL(baseUrl).hostname.replace(/^\[/, '').replace(/\]$/, '')
  if (isIP(hostname) && isPrivateIp(hostname) && !connection.allowPrivate) {
    return errAsync({ _tag: 'CiPrivateOriginApprovalRequired', baseUrl })
  }
  const requestInit: RequestInit & { dispatcher: Agent } = {
    method: init?.method ?? 'GET',
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: 'application/json',
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
    },
    body: init?.body,
    redirect: 'error',
    signal: AbortSignal.timeout(15_000),
    dispatcher: dispatcherFor(connection),
  }
  return fromExternalCall(fetch(url, requestInit), (e) =>
    isPrivateOriginBlocked(e)
      ? { _tag: 'CiPrivateOriginApprovalRequired' as const, baseUrl }
      : init?.ambiguousDispatchUrl
        ? ambiguousDispatchError(init.ambiguousDispatchUrl)
        : apiError(0, errorMessage(e), token),
  ).andThen((res) => {
    const contentLength = Number(res.headers.get('content-length'))
    if (Number.isFinite(contentLength) && contentLength > MAX_RESPONSE_BYTES) {
      const oversizedError =
        init?.ambiguousDispatchUrl && (res.ok || res.status >= 500)
          ? ambiguousDispatchError(init.ambiguousDispatchUrl)
          : apiError(res.ok ? 0 : res.status, 'TeamCity response exceeds the size limit')
      return ResultAsync.fromPromise(
        res.body?.cancel() ?? Promise.resolve(),
        (): CiError => oversizedError,
      ).andThen(() => errAsync(oversizedError))
    }

    return fromExternalCall(readBoundedResponse(res), (e) =>
      init?.ambiguousDispatchUrl && (res.ok || res.status >= 500)
        ? ambiguousDispatchError(init.ambiguousDispatchUrl)
        : apiError(res.ok ? 0 : res.status, errorMessage(e), token),
    ).andThen((bytes) => {
      const body = Buffer.from(bytes).toString('utf8')
      if (!res.ok) {
        if (init?.ambiguousDispatchUrl && res.status >= 500) {
          return errAsync(ambiguousDispatchError(init.ambiguousDispatchUrl))
        }
        return errAsync(
          apiError(
            res.status,
            init?.method && init.method !== 'GET'
              ? 'TeamCity rejected the request'
              : redactTeamCityToken(body.trim(), token).slice(0, 300) || res.statusText,
          ),
        )
      }

      if (!body.trim()) {
        return errAsync(
          init?.ambiguousDispatchUrl
            ? ambiguousDispatchError(init.ambiguousDispatchUrl)
            : apiError(0, 'TeamCity returned malformed JSON'),
        )
      }

      try {
        return okAsync(JSON.parse(body) as T)
      } catch {
        return errAsync(
          init?.ambiguousDispatchUrl
            ? ambiguousDispatchError(init.ambiguousDispatchUrl)
            : apiError(0, 'TeamCity returned malformed JSON'),
        )
      }
    })
  })
}

/** Cheap authenticated probe used by the Settings connection test. */
export function testConnection(
  baseUrl: string,
  token: string,
  connection: TeamCityConnectionPolicy,
): ResultAsync<void, CiError> {
  return tcFetch<unknown>(baseUrl, token, connection, '/app/rest/server')
    .andThen((response) =>
      decodeTcResponse<JsonRecord>(
        response,
        (value) => isRecord(value) && typeof value.version === 'string' && value.version.length > 0,
      ),
    )
    .map(() => undefined)
}

/** Newest build (queued/running included) of a configuration on a branch, or null. */
export function fetchBuildForBranch(
  baseUrl: string,
  token: string,
  connection: TeamCityConnectionPolicy,
  buildTypeId: string,
  branch: string,
): ResultAsync<CiBuildStatus | null, CiError> {
  const locator = encodeURIComponent(buildBranchLocator(buildTypeId, branch))
  return tcFetch<unknown>(
    baseUrl,
    token,
    connection,
    `/app/rest/builds?locator=${locator}&fields=count,build(${BUILD_FIELDS})`,
  )
    .andThen((response) =>
      decodeTcResponse<{ count?: number; build?: RawBuild[] }>(response, isBuildCollection),
    )
    .map(parseBuildsResponse)
}

/** Single build by id — used to watch a build triggered from Canopy to completion. */
export function fetchBuild(
  baseUrl: string,
  token: string,
  connection: TeamCityConnectionPolicy,
  buildId: number,
): ResultAsync<CiBuildWithType, CiError> {
  return tcFetch<unknown>(
    baseUrl,
    token,
    connection,
    `/app/rest/builds/id:${buildId}?fields=${BUILD_FIELDS},buildType(id)`,
  )
    .andThen((response) => decodeTcResponse<RawBuild>(response, isRawBuild))
    .andThen((raw) =>
      raw.buildType?.id
        ? okAsync({ ...mapBuild(raw), buildTypeId: raw.buildType.id })
        : errAsync(apiError(0, 'TeamCity returned a build without its build configuration')),
    )
}

/** Queue a build of the configuration on the given branch, optionally with custom parameters. */
export function triggerBuild(
  baseUrl: string,
  token: string,
  connection: TeamCityConnectionPolicy,
  buildTypeId: string,
  branch: string,
  properties?: Array<{ name: string; value: string }>,
): ResultAsync<CiTriggerResult, CiError> {
  const detailsUrl = `${baseUrl.replace(/\/$/, '')}/viewType.html?buildTypeId=${encodeURIComponent(buildTypeId)}`
  return tcFetch<unknown>(baseUrl, token, connection, '/app/rest/buildQueue', {
    method: 'POST',
    ambiguousDispatchUrl: detailsUrl,
    body: JSON.stringify({
      buildType: { id: buildTypeId },
      branchName: branch,
      comment: { text: 'Triggered from Canopy' },
      ...(properties?.length ? { properties: { property: properties } } : {}),
    }),
  })
    .andThen((response) =>
      decodeTcResponse<{ id: number; webUrl?: string; branchName?: string }>(
        response,
        (value) =>
          isRecord(value) &&
          typeof value.id === 'number' &&
          hasOptionalString(value, 'webUrl') &&
          hasOptionalString(value, 'branchName'),
      ),
    )
    .orElse((error) =>
      error._tag === 'CiApiError' && error.status === 0
        ? errAsync(ambiguousDispatchError(detailsUrl))
        : errAsync(error),
    )
    .map((res) => ({ buildId: res.id, webUrl: res.webUrl ?? '', branchName: res.branchName }))
}

/** All build configurations on the server — source for the per-repo config picker. */
export function fetchBuildTypes(
  baseUrl: string,
  token: string,
  connection: TeamCityConnectionPolicy,
): ResultAsync<CiServerBuildType[], CiError> {
  return tcFetch<unknown>(
    baseUrl,
    token,
    connection,
    '/app/rest/buildTypes?fields=buildType(id,name,projectName)',
  )
    .andThen((response) =>
      decodeTcResponse<{
        buildType?: Array<{ id?: string; name?: string; projectName?: string }>
      }>(response, (value) => isCollection(value, 'buildType', isBuildType)),
    )
    .map((res) =>
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
  connection: TeamCityConnectionPolicy,
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
    tcFetch<unknown>(baseUrl, token, connection, path)
      .andThen((response) => decodeTcResponse<RawActivityResponse>(response, isBuildCollection))
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
  connection: TeamCityConnectionPolicy,
  buildTypeId: string,
): ResultAsync<string[], CiError> {
  return tcFetch<unknown>(
    baseUrl,
    token,
    connection,
    `/app/rest/buildTypes/id:${buildTypeId}/branches?locator=policy:VCS_BRANCHES&fields=branch(name,default)`,
  )
    .andThen((response) =>
      decodeTcResponse<{
        count?: number
        branch?: Array<{ name?: string; default?: boolean }>
      }>(response, (value) => isCollection(value, 'branch', isBranch)),
    )
    .map(parseBranches)
}

/** The parameters TeamCity would prompt for in its "Run custom build" dialog. */
export function fetchPromptParameters(
  baseUrl: string,
  token: string,
  connection: TeamCityConnectionPolicy,
  buildTypeId: string,
): ResultAsync<CiParameter[], CiError> {
  return tcFetch<unknown>(
    baseUrl,
    token,
    connection,
    `/app/rest/buildTypes/id:${buildTypeId}/parameters?fields=property(name,value,type(rawValue))`,
  )
    .andThen((response) =>
      decodeTcResponse<{
        count?: number
        property?: Array<{ name: string; value?: string; type?: { rawValue?: string } }>
      }>(response, (value) => isCollection(value, 'property', isParameter)),
    )
    .map(parsePromptParameters)
}
