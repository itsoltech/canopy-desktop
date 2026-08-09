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
import { withCiDegradedCauses } from '../degraded'

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

  private resolveRef(ref: CiRef): ResultAsync<CiRef, CiError> {
    const otherKind = ref.kind === 'branch' ? 'tag' : 'branch'
    return ResultAsync.combine([
      this.client.getExactRef(ref.kind, ref.name),
      this.client.getExactRef(otherKind, ref.name),
    ]).andThen(([resolved, collision]) => {
      if (!resolved) return err(apiError(`${ref.kind} ${ref.name} no longer exists on GitHub`))
      if (collision) {
        return err(apiError(`Branch and tag both use the name ${ref.name}; dispatch is ambiguous`))
      }
      return ok({ ...ref, commitSha: resolved.commitSha })
    })
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
            const rows: CiJobStatus[] = []
            const causes: CiError[] = []
            for (const configured of this.config.workflows) {
              const workflow = workflows.find(
                (candidate) => candidate.path.toLowerCase() === configured.path.toLowerCase(),
              )
              if (!workflow) {
                rows.push({
                  jobId: configured.path,
                  label: configured.label,
                  provider: 'github-actions',
                  run: null,
                  error: `Configured workflow ${configured.path} is missing`,
                })
                continue
              }
              const runs = await this.client.listWorkflowRuns(workflow.id, ref.name)
              if (runs.isErr() && runs.error._tag === 'CiRateLimited') return err(runs.error)
              if (runs.isErr()) causes.push(runs.error)
              rows.push(
                runs.isOk()
                  ? {
                      jobId: configured.path,
                      label: configured.label,
                      provider: 'github-actions',
                      run: runs.value[0]
                        ? mapGitHubRun(runs.value[0], configured.path, configured.label)
                        : null,
                    }
                  : {
                      jobId: configured.path,
                      label: configured.label,
                      provider: 'github-actions',
                      run: null,
                      error: ciErrorMessage(runs.error),
                    },
              )
            }
            return ok(causes.length > 0 ? withCiDegradedCauses(rows, causes) : rows)
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
            const runs: CiRun[] = []
            const partialErrors: string[] = []
            const causes: CiError[] = []
            for (const configured of this.config.workflows) {
              const workflow = workflows.find(
                (candidate) => candidate.path.toLowerCase() === configured.path.toLowerCase(),
              )
              if (!workflow) {
                partialErrors.push(`Configured workflow ${configured.path} is missing`)
                continue
              }
              // `branch` narrows the query itself, not the response: `recent` is sliced to
              // the ten newest across every configured workflow, so a response-side filter
              // would drop a branch whose last run is older than that.
              const result = await this.client.listWorkflowRunsPage(workflow.id, branch)
              if (result.isErr()) {
                if (result.error._tag === 'CiRateLimited') return err(result.error)
                causes.push(result.error)
                partialErrors.push(`${configured.label}: ${ciErrorMessage(result.error)}`)
                continue
              }
              runs.push(
                ...result.value.runs.map((run) =>
                  mapGitHubRun(run, configured.path, configured.label),
                ),
              )
            }
            runs.sort(newestFirst)
            const activity: CiRunActivity = {
              running: runs.filter((run) => run.state === 'running' || run.state === 'waiting'),
              queued: runs.filter((run) => run.state === 'queued'),
              recent: runs.filter((run) => run.state === 'finished').slice(0, 10),
              ...(partialErrors.length ? { partialErrors } : {}),
            }
            return ok(causes.length > 0 ? withCiDegradedCauses(activity, causes) : activity)
          })(),
        ),
    )
  }

  run(runId: string): ResultAsync<CiRun, CiError> {
    return this.authenticated(() =>
      this.client.getRun(runId).map((raw: GitHubWorkflowRun) => {
        const path = raw.path?.split('@')[0]
        const configured = this.config.workflows.find((workflow) => workflow.path === path)
        return mapGitHubRun(
          raw,
          configured?.path ?? path ?? '',
          configured?.label ?? raw.name ?? 'Run',
        )
      }),
    )
  }
}
