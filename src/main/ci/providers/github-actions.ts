import { ResultAsync, err, errAsync, ok, type Result } from 'neverthrow'
import { ciErrorMessage, type CiError } from '../errors'
import type {
  CiJobStatus,
  CiDiscoveredWorkflow,
  CiParameter,
  CiParameterSet,
  CiRef,
  CiRun,
  CiRunActivity,
  CiRunTriggerResult,
  CiTriggerRequest,
  GitHubActionsCiConfig,
} from '../types'
import {
  GitHubActionsClient,
  type GitHubWorkflow,
  type GitHubWorkflowRun,
} from '../github-actions/client'
import { mapGitHubRun } from '../github-actions/mappers'
import {
  parseWorkflowDispatch,
  validateWorkflowInputs,
  type GitHubWorkflowSchema,
} from '../github-actions/workflow'
import type { CiProviderAdapter } from './types'

export function discoverGitHubWorkflows(
  client: GitHubActionsClient,
  defaultBranch: string,
): ResultAsync<CiDiscoveredWorkflow[], CiError> {
  return new ResultAsync(
    (async (): Promise<Result<CiDiscoveredWorkflow[], CiError>> => {
      const workflows = await client.listWorkflows()
      if (workflows.isErr()) return err(workflows.error)
      const discovered: CiDiscoveredWorkflow[] = []
      for (const workflow of workflows.value.filter((item) => item.state === 'active')) {
        const file = await client.getWorkflowFile(workflow.path, defaultBranch)
        if (file.isErr()) {
          discovered.push({
            id: String(workflow.id),
            path: workflow.path,
            name: workflow.name,
            webUrl: workflow.htmlUrl,
            available: false,
            error: ciErrorMessage(file.error),
          })
          continue
        }
        const parsed = parseWorkflowDispatch(file.value.content)
        discovered.push({
          id: String(workflow.id),
          path: workflow.path,
          name: workflow.name,
          webUrl: workflow.htmlUrl,
          available: parsed.isOk(),
          ...(parsed.isErr() ? { error: parsed.error.reason } : {}),
        })
      }
      return ok(discovered)
    })(),
  )
}

function apiError(message: string): CiError {
  return { _tag: 'CiApiError', status: 0, message, provider: 'github-actions' }
}

function schemaError(reason: string): CiError {
  return { _tag: 'CiWorkflowSchemaInvalid', reason }
}

function parameter(
  input: GitHubWorkflowSchema['inputs'][number],
  environments: string[],
): CiParameter {
  const options = input.type === 'environment' ? environments : input.options
  return {
    name: input.name,
    kind:
      input.type === 'boolean'
        ? 'checkbox'
        : input.type === 'choice' || input.type === 'environment'
          ? 'select'
          : 'text',
    label: input.label,
    description: input.description,
    required: input.required,
    defaultValue:
      input.defaultValue === undefined
        ? ''
        : input.defaultValue === true
          ? 'true'
          : String(input.defaultValue),
    options,
    multiple: false,
    valueSeparator: ',',
    checkedValue: input.type === 'boolean' ? 'true' : undefined,
    uncheckedValue: input.type === 'boolean' ? 'false' : undefined,
    valueType: input.type === 'boolean' ? 'boolean' : 'string',
    hasDefault: input.defaultValue !== undefined,
  }
}

function newestFirst(a: CiRun, b: CiRun): number {
  return (b.queuedAt ?? 0) - (a.queuedAt ?? 0)
}

function workflowPath(run: GitHubWorkflowRun): string | undefined {
  return run.path?.split('@')[0]
}

export class GitHubActionsAdapter implements CiProviderAdapter {
  constructor(
    private readonly config: GitHubActionsCiConfig,
    private readonly client: GitHubActionsClient,
  ) {}

  private authenticated<T>(operation: () => ResultAsync<T, CiError>): ResultAsync<T, CiError> {
    return this.client.verifyAuthentication().andThen(operation)
  }

  private configuredWorkflow(
    jobId: string,
  ): Result<GitHubActionsCiConfig['workflows'][number], CiError> {
    const configured = this.config.workflows.find((workflow) => workflow.path === jobId)
    return configured
      ? ok(configured)
      : err(apiError(`Workflow ${jobId} is not configured for this repository`))
  }

  private workflow(jobId: string): ResultAsync<GitHubWorkflow, CiError> {
    const configured = this.configuredWorkflow(jobId)
    if (configured.isErr()) return errAsync(configured.error)
    return this.client.listWorkflows().andThen((workflows) => {
      const workflow = workflows.find(
        (candidate) =>
          candidate.path.toLowerCase() === configured.value.path.toLowerCase() &&
          candidate.state === 'active',
      )
      return workflow
        ? ok(workflow)
        : err(apiError(`Configured workflow ${configured.value.path} is missing or disabled`))
    })
  }

  private resolveRefName(name: string): ResultAsync<CiRef, CiError> {
    return ResultAsync.combine([
      this.client.getExactRef('branch', name),
      this.client.getExactRef('tag', name),
    ]).andThen(([branch, tag]) => {
      if (branch && tag) {
        return err(apiError(`Branch and tag both use the name ${name}; dispatch is ambiguous`))
      }
      if (branch) return ok({ ...branch, kind: 'branch' as const })
      if (tag) return ok({ ...tag, kind: 'tag' as const })
      return err(apiError(`Ref ${name} no longer exists on GitHub`))
    })
  }

  private resolveRef(ref: CiRef): ResultAsync<CiRef, CiError> {
    return this.resolveRefName(ref.name).andThen((resolved) =>
      resolved.kind === ref.kind
        ? ok(resolved)
        : err(apiError(`${ref.kind} ${ref.name} no longer exists on GitHub`)),
    )
  }

  refs(jobId: string): ResultAsync<CiRef[], CiError> {
    const configured = this.configuredWorkflow(jobId)
    if (configured.isErr()) return errAsync(configured.error)
    return this.authenticated(() =>
      ResultAsync.combine([this.client.listBranches(), this.client.listTags()]).map(
        ([branches, tags]) => [
          ...branches.map((ref): CiRef => ({ ...ref, kind: 'branch' })),
          ...tags.map((ref): CiRef => ({ ...ref, kind: 'tag' })),
        ],
      ),
    )
  }

  exactRef(jobId: string, name: string): ResultAsync<CiRef, CiError> {
    const configured = this.configuredWorkflow(jobId)
    if (configured.isErr()) return errAsync(configured.error)
    return this.authenticated(() => this.resolveRefName(name))
  }

  parameters(jobId: string, ref: CiRef): ResultAsync<CiParameterSet, CiError> {
    const configured = this.configuredWorkflow(jobId)
    if (configured.isErr()) return errAsync(configured.error)
    return this.authenticated(() =>
      this.resolveRef(ref).andThen((resolvedRef) =>
        this.client.getWorkflowFile(jobId, resolvedRef.name).andThen((file) => {
          const parsed = parseWorkflowDispatch(file.content)
          if (parsed.isErr()) return errAsync(schemaError(parsed.error.reason))
          if (!parsed.value.inputs.some((input) => input.type === 'environment')) {
            return ResultAsync.fromSafePromise(
              Promise.resolve({
                schemaRevision: file.sha,
                parameters: parsed.value.inputs.map((input) => parameter(input, [])),
              }),
            )
          }
          return this.client.listEnvironments().map((environments) => ({
            schemaRevision: file.sha,
            parameters: parsed.value.inputs.map((input) =>
              parameter(
                input,
                environments.map((environment) => environment.name),
              ),
            ),
          }))
        }),
      ),
    )
  }

  trigger(request: CiTriggerRequest): ResultAsync<CiRunTriggerResult, CiError> {
    return this.authenticated(() =>
      this.workflow(request.jobId).andThen((workflow) =>
        this.resolveRef(request.ref).andThen((resolvedRef) =>
          this.client.getWorkflowFile(request.jobId, resolvedRef.name).andThen((file) => {
            if (request.ref.commitSha && resolvedRef.commitSha !== request.ref.commitSha) {
              return errAsync<CiRunTriggerResult, CiError>({ _tag: 'CiRefChanged' })
            }
            if (!request.schemaRevision || file.sha !== request.schemaRevision) {
              return errAsync<CiRunTriggerResult, CiError>({ _tag: 'CiWorkflowSchemaChanged' })
            }
            const parsed = parseWorkflowDispatch(file.content)
            if (parsed.isErr()) return errAsync(schemaError(parsed.error.reason))
            const validated = validateWorkflowInputs(parsed.value, request.inputs)
            if (validated.isErr()) return errAsync(schemaError(validated.error.reason))

            const dispatch = (environments: string[]): ResultAsync<CiRunTriggerResult, CiError> => {
              for (const input of parsed.value.inputs) {
                if (input.type !== 'environment') continue
                const value = request.inputs[input.name]
                if (typeof value === 'string' && !environments.includes(value)) {
                  return errAsync(schemaError(`workflow input ${input.name} is not an environment`))
                }
              }
              return this.client
                .dispatchWorkflow(workflow.id, resolvedRef.name, validated.value)
                .map((result) => ({
                  provider: 'github-actions',
                  runId: result.runId,
                  webUrl: result.webUrl,
                  ref: resolvedRef,
                }))
            }

            return parsed.value.inputs.some((input) => input.type === 'environment')
              ? this.client
                  .listEnvironments()
                  .andThen((items) => dispatch(items.map((item) => item.name)))
              : dispatch([])
          }),
        ),
      ),
    )
  }

  status(ref: CiRef): ResultAsync<CiJobStatus[], CiError> {
    return this.authenticated(
      () =>
        new ResultAsync(
          (async (): Promise<Result<CiJobStatus[], CiError>> => {
            const workflowsResult = await this.client.listWorkflows()
            if (workflowsResult.isErr()) return err(workflowsResult.error)
            const workflows = workflowsResult.value
            const runsResult = await this.client.listRepositoryRuns(ref.name)
            if (runsResult.isErr()) return err(runsResult.error)
            const snapshotTruncated = runsResult.value.totalCount > runsResult.value.runs.length
            const rows: CiJobStatus[] = this.config.workflows.map((configured) => {
              const workflow = workflows.find(
                (candidate) => candidate.path.toLowerCase() === configured.path.toLowerCase(),
              )
              if (!workflow) {
                return {
                  jobId: configured.path,
                  label: configured.label,
                  provider: 'github-actions',
                  run: null,
                  error: `Configured workflow ${configured.path} is missing`,
                }
              }
              const run = runsResult.value.runs.find(
                (candidate) =>
                  workflowPath(candidate)?.toLowerCase() === configured.path.toLowerCase(),
              )
              return {
                jobId: configured.path,
                label: configured.label,
                provider: 'github-actions',
                run: run ? mapGitHubRun(run, configured.path, configured.label) : null,
                ...(!run && snapshotTruncated
                  ? { error: `Older runs for ${configured.label} are outside the bounded history` }
                  : {}),
              }
            })
            return ok(rows)
          })(),
        ),
    )
  }

  activity(branch?: string): ResultAsync<CiRunActivity, CiError> {
    return this.authenticated(
      () =>
        new ResultAsync(
          (async (): Promise<Result<CiRunActivity, CiError>> => {
            const workflowsResult = await this.client.listWorkflows()
            if (workflowsResult.isErr()) return err(workflowsResult.error)
            const workflows = workflowsResult.value
            const partialErrors: string[] = []
            for (const configured of this.config.workflows) {
              const workflow = workflows.find(
                (candidate) => candidate.path.toLowerCase() === configured.path.toLowerCase(),
              )
              if (!workflow) {
                partialErrors.push(`Configured workflow ${configured.path} is missing`)
              }
            }
            // The branch is part of the repository query. Filtering only after a global
            // response would hide a branch whose newest run is older than another branch's.
            const result = await this.client.listRepositoryRuns(branch)
            if (result.isErr()) return err(result.error)
            const configuredByPath = new Map(
              this.config.workflows.map((workflow) => [workflow.path.toLowerCase(), workflow]),
            )
            const runs: CiRun[] = result.value.runs.flatMap((run) => {
              const path = workflowPath(run)
              const configured = path ? configuredByPath.get(path.toLowerCase()) : undefined
              return configured ? [mapGitHubRun(run, configured.path, configured.label)] : []
            })
            if (result.value.totalCount > result.value.runs.length) {
              partialErrors.push('Older workflow runs are outside the bounded history')
            }
            runs.sort(newestFirst)
            const activity: CiRunActivity = {
              running: runs.filter((run) => run.state === 'running' || run.state === 'waiting'),
              queued: runs.filter((run) => run.state === 'queued'),
              recent: runs.filter((run) => run.state === 'finished').slice(0, 10),
              ...(partialErrors.length ? { partialErrors } : {}),
            }
            return ok(activity)
          })(),
        ),
    )
  }

  run(runId: string): ResultAsync<CiRun, CiError> {
    return this.authenticated(() =>
      this.client.getRun(runId).andThen((raw: GitHubWorkflowRun) => {
        const path = raw.path?.split('@')[0]
        const configured = this.config.workflows.find((workflow) => workflow.path === path)
        return configured
          ? ok(mapGitHubRun(raw, configured.path, configured.label))
          : err(apiError(`Run ${runId} does not belong to a configured workflow`))
      }),
    )
  }
}
