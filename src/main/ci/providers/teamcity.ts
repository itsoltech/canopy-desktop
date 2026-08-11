import { ResultAsync, errAsync, okAsync } from 'neverthrow'
import { ciErrorMessage, type CiError } from '../errors'
import {
  fetchActivity,
  fetchBranches,
  fetchBuild,
  fetchBuildForBranch,
  fetchPromptParameters,
  isTeamCityLocatorSafeRef,
  triggerBuild,
} from '../teamcity'
import type {
  CiActivityBuild,
  CiBuildStatus,
  CiJobStatus,
  CiParameterSet,
  CiRef,
  CiRun,
  CiRunActivity,
  CiRunConclusion,
  CiRunTriggerResult,
  CiTriggerRequest,
  TeamCityCiConfig,
} from '../types'
import type { CiProviderAdapter } from './types'
import { ciDegradedCauses, withCiDegradedCauses } from '../degraded'

interface TeamCityClient {
  fetchBuildForBranch: typeof fetchBuildForBranch
  fetchBuild: typeof fetchBuild
  fetchActivity: typeof fetchActivity
  fetchBranches: typeof fetchBranches
  fetchPromptParameters: typeof fetchPromptParameters
  triggerBuild: typeof triggerBuild
}

const defaultClient: TeamCityClient = {
  fetchBuildForBranch,
  fetchBuild,
  fetchActivity,
  fetchBranches,
  fetchPromptParameters,
  triggerBuild,
}

function conclusion(status: string | undefined): CiRunConclusion {
  if (status === 'SUCCESS') return 'success'
  if (status === 'FAILURE' || status === 'ERROR') return 'failure'
  return 'unknown'
}

function mapBuild(build: CiBuildStatus, jobId: string, jobLabel: string): CiRun {
  return {
    provider: 'teamcity',
    runId: String(build.id),
    number: build.number,
    jobId,
    jobLabel,
    state: build.state,
    conclusion: conclusion(build.status),
    statusText: build.statusText,
    webUrl: build.webUrl,
    ref: build.branchName ? { name: build.branchName, kind: 'branch' } : undefined,
    queuedAt: build.queuedAt,
    startedAt: build.startedAt,
    finishedAt: build.finishedAt,
  }
}

function mapActivityBuild(build: CiActivityBuild): CiRun {
  return {
    provider: 'teamcity',
    runId: String(build.id),
    number: build.number,
    jobId: build.buildTypeId,
    jobLabel: build.buildTypeName,
    state: build.state,
    conclusion: conclusion(build.status),
    statusText: build.statusText,
    webUrl: build.webUrl,
    ref: build.branchName ? { name: build.branchName, kind: 'branch' } : undefined,
    queuedAt: build.queuedAt,
    startedAt: build.startedAt,
    finishedAt: build.finishedAt,
  }
}

export class TeamCityAdapter implements CiProviderAdapter {
  private readonly client: TeamCityClient

  constructor(
    private readonly config: TeamCityCiConfig,
    private readonly token: string,
    client: Partial<TeamCityClient> = {},
  ) {
    this.client = { ...defaultClient, ...client }
  }

  private buildType(jobId: string): TeamCityCiConfig['buildTypes'][number] | undefined {
    return this.config.buildTypes.find((buildType) => buildType.id === jobId)
  }

  status(ref: CiRef): ResultAsync<CiJobStatus[], CiError> {
    if (ref.kind !== 'branch')
      return errAsync({ _tag: 'CiApiError', status: 0, message: 'TeamCity requires a branch' })
    if (!isTeamCityLocatorSafeRef(ref.name)) {
      return errAsync({
        _tag: 'CiApiError',
        status: 0,
        message: 'TeamCity branch contains locator-unsafe characters',
      })
    }
    const causes: CiError[] = []
    return ResultAsync.combine(
      this.config.buildTypes.map((buildType) =>
        this.client
          .fetchBuildForBranch(this.config.baseUrl, this.token, buildType.id, ref.name)
          .map((build): CiJobStatus => ({
            jobId: buildType.id,
            label: buildType.label,
            provider: 'teamcity',
            run: build ? mapBuild(build, buildType.id, buildType.label) : null,
          }))
          .orElse((error) => {
            causes.push(error)
            return okAsync({
              jobId: buildType.id,
              label: buildType.label,
              provider: 'teamcity' as const,
              run: null,
              error: ciErrorMessage(error),
            })
          }),
      ),
    ).map((rows) => (causes.length > 0 ? withCiDegradedCauses(rows, causes) : rows))
  }

  refs(jobId: string): ResultAsync<CiRef[], CiError> {
    if (!this.buildType(jobId))
      return errAsync({
        _tag: 'CiApiError',
        status: 0,
        message: `Build type ${jobId} is not configured`,
      })
    return this.client
      .fetchBranches(this.config.baseUrl, this.token, jobId)
      .map((branches) => branches.map((name): CiRef => ({ name, kind: 'branch' })))
  }

  exactRef(jobId: string, name: string): ResultAsync<CiRef, CiError> {
    return this.refs(jobId).andThen((refs) => {
      const ref = refs.find((candidate) => candidate.name === name)
      return ref
        ? okAsync(ref)
        : errAsync<CiRef, CiError>({
            _tag: 'CiApiError',
            status: 0,
            message: `Branch ${name} is not available in TeamCity`,
          })
    })
  }

  parameters(jobId: string): ResultAsync<CiParameterSet, CiError> {
    if (!this.buildType(jobId))
      return errAsync({
        _tag: 'CiApiError',
        status: 0,
        message: `Build type ${jobId} is not configured`,
      })
    return this.client
      .fetchPromptParameters(this.config.baseUrl, this.token, jobId)
      .map((parameters) => ({ parameters, schemaRevision: `teamcity:${jobId}` }))
  }

  trigger(request: CiTriggerRequest): ResultAsync<CiRunTriggerResult, CiError> {
    if (!this.buildType(request.jobId))
      return errAsync({
        _tag: 'CiApiError',
        status: 0,
        message: `Build type ${request.jobId} is not configured`,
      })
    if (request.ref.kind !== 'branch')
      return errAsync({ _tag: 'CiApiError', status: 0, message: 'TeamCity requires a branch' })
    const properties: Array<{ name: string; value: string }> = []
    for (const [name, value] of Object.entries(request.inputs)) {
      if (typeof value !== 'string') {
        return errAsync({
          _tag: 'CiApiError',
          status: 0,
          message: `TeamCity property ${name} must be text`,
        })
      }
      properties.push({ name, value })
    }
    return this.client
      .triggerBuild(this.config.baseUrl, this.token, request.jobId, request.ref.name, properties)
      .map((result) => ({
        provider: 'teamcity',
        runId: String(result.buildId),
        webUrl: result.webUrl,
        ref: {
          name: result.branchName ?? request.ref.name,
          kind: 'branch',
        },
      }))
  }

  run(runId: string): ResultAsync<CiRun, CiError> {
    if (!/^\d+$/.test(runId))
      return errAsync({ _tag: 'CiApiError', status: 0, message: 'Invalid TeamCity build id' })
    return this.client
      .fetchBuild(this.config.baseUrl, this.token, Number(runId))
      .andThen((build) => {
        const configured = this.config.buildTypes.find((item) => item.id === build.buildTypeId)
        return configured
          ? okAsync(mapBuild(build, configured.id, configured.label))
          : errAsync({
              _tag: 'CiApiError' as const,
              status: 403,
              message: 'Build does not belong to a configured TeamCity job',
              provider: 'teamcity' as const,
            })
      })
  }

  activity(branch?: string): ResultAsync<CiRunActivity, CiError> {
    return this.client
      .fetchActivity(
        this.config.baseUrl,
        this.token,
        this.config.buildTypes.map((buildType) => buildType.id),
        branch,
      )
      .map((activity) => {
        const result: CiRunActivity = {
          running: activity.running.map(mapActivityBuild),
          queued: activity.queued.map(mapActivityBuild),
          recent: activity.recent.map(mapActivityBuild),
          ...(activity.partialErrors?.length ? { partialErrors: activity.partialErrors } : {}),
        }
        const causes = ciDegradedCauses(activity)
        return causes === undefined ? result : withCiDegradedCauses(result, causes)
      })
  }
}
