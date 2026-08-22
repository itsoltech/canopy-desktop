import { createHash } from 'crypto'
import { ResultAsync, err, ok, type Result } from 'neverthrow'
import type { CiError, CiError as GitHubClientError } from '../errors'

const API_ORIGIN = 'https://api.github.com'
const API_VERSION = '2026-03-10'
const REQUEST_TIMEOUT_MS = 15_000
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024
const MAX_PAGES = 5
const rateLimitedUntil = new Map<string, number>()

function pruneExpiredRateLimits(now: number): void {
  for (const [key, resetAt] of rateLimitedUntil) {
    if (resetAt <= now) rateLimitedUntil.delete(key)
  }
}

export interface GitHubWorkflow {
  id: number
  name: string
  path: string
  state: string
  htmlUrl: string
}

export interface GitHubWorkflowFile {
  sha: string
  content: string
}

export interface GitHubDispatchResult {
  runId: string
  apiUrl: string
  webUrl: string
}

export interface GitHubRefResult {
  name: string
  commitSha: string
}

export interface GitHubEnvironment {
  name: string
}

export interface GitHubRepositoryInfo {
  fullName: string
  defaultBranch: string
}

export interface GitHubWorkflowRun {
  id: number
  run_number?: number
  run_attempt?: number
  name?: string
  display_title?: string
  path?: string
  status?: string
  conclusion?: string | null
  html_url?: string
  head_branch?: string | null
  head_sha?: string
  event?: string
  created_at?: string
  run_started_at?: string | null
  updated_at?: string
  actor?: { login?: string }
}

export interface GitHubRepositoryRunsOptions {
  maxPages?: number
  stopWhen?: (runs: GitHubWorkflowRun[]) => boolean
}

interface RawWorkflow {
  id?: number
  name?: string
  path?: string
  state?: string
  html_url?: string
}

interface RawWorkflowFile {
  sha?: string
  encoding?: string
  content?: string
  type?: string
}

interface GitHubRequestOptions {
  method?: 'GET' | 'POST'
  body?: unknown
  /** A transport/decode failure after this mutation starts has an unknown outcome. */
  ambiguousWorkflowUrl?: string
}

function apiError(status: number, message: string): CiError {
  return { _tag: 'CiApiError', status, message, provider: 'github-actions' }
}

function safeStatusMessage(response: Response): string {
  return response.statusText || 'Request failed'
}

function rateLimitResetAt(response: Response): number | undefined {
  const resetSeconds = Number(response.headers.get('x-ratelimit-reset'))
  if (Number.isFinite(resetSeconds) && resetSeconds > 0) return resetSeconds * 1000
  const retryAfter = response.headers.get('retry-after')
  if (retryAfter) {
    const seconds = Number(retryAfter)
    if (Number.isFinite(seconds) && seconds >= 0) return Date.now() + seconds * 1000
    const date = Date.parse(retryAfter)
    if (!Number.isNaN(date)) return date
  }
  return undefined
}

export class GitHubActionsClient {
  private readonly repositoryPath: string
  private readonly rateLimitKey: string

  constructor(
    owner: string,
    repository: string,
    private readonly token: string,
  ) {
    this.repositoryPath = `/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repository)}`
    // A replacement credential must get one real request of its own. It may still share a
    // user-level GitHub budget and be limited again, but a backoff earned by a different token
    // must not suppress the request locally. Hashing avoids retaining another plaintext copy.
    const credentialFingerprint = createHash('sha256').update(token).digest('base64url')
    this.rateLimitKey = `${this.repositoryPath}:${credentialFingerprint}`
  }

  private request<T>(path: string, init?: GitHubRequestOptions): ResultAsync<T, GitHubClientError> {
    return new ResultAsync(this.performRequest<T>(`${this.repositoryPath}${path}`, init))
  }

  private apiRequest<T>(path: string): ResultAsync<T, GitHubClientError> {
    return new ResultAsync(this.performRequest<T>(path))
  }

  private async performRequest<T>(
    requestPath: string,
    init?: GitHubRequestOptions,
  ): Promise<Result<T, GitHubClientError>> {
    const now = Date.now()
    pruneExpiredRateLimits(now)
    const limitedUntil = rateLimitedUntil.get(this.rateLimitKey)
    if (limitedUntil !== undefined && limitedUntil > now) {
      return err({ _tag: 'CiRateLimited', resetAt: limitedUntil })
    }

    let response: Response
    try {
      response = await fetch(`${API_ORIGIN}${requestPath}`, {
        method: init?.method ?? 'GET',
        headers: {
          Accept: 'application/vnd.github+json',
          Authorization: `Bearer ${this.token}`,
          'X-GitHub-Api-Version': API_VERSION,
          ...(init?.body === undefined ? {} : { 'Content-Type': 'application/json' }),
        },
        ...(init?.body === undefined ? {} : { body: JSON.stringify(init.body) }),
        redirect: 'error',
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      })
    } catch (cause) {
      if (init?.ambiguousWorkflowUrl) {
        return err({ _tag: 'CiDispatchAmbiguous', workflowUrl: init.ambiguousWorkflowUrl })
      }
      const message =
        cause instanceof Error ? cause.message.slice(0, 300) : 'Network request failed'
      return err(apiError(0, message.replaceAll(this.token, '[redacted]')))
    }

    const isRateLimited =
      response.status === 429 ||
      (response.status === 403 &&
        (response.headers.get('x-ratelimit-remaining') === '0' ||
          response.headers.has('retry-after')))
    if (isRateLimited) {
      const resetAt = rateLimitResetAt(response) ?? Date.now() + 60_000
      if (resetAt > Date.now()) {
        rateLimitedUntil.set(this.rateLimitKey, resetAt)
      }
      return err({
        _tag: 'CiRateLimited',
        resetAt,
      })
    }
    if (!response.ok) return err(apiError(response.status, safeStatusMessage(response)))

    const contentLength = Number(response.headers.get('content-length'))
    if (Number.isFinite(contentLength) && contentLength > MAX_RESPONSE_BYTES) {
      if (init?.ambiguousWorkflowUrl) {
        return err({ _tag: 'CiDispatchAmbiguous', workflowUrl: init.ambiguousWorkflowUrl })
      }
      return err(apiError(0, 'GitHub response exceeds the size limit'))
    }
    let bytes: ArrayBuffer
    try {
      bytes = await response.arrayBuffer()
    } catch (cause) {
      if (init?.ambiguousWorkflowUrl) {
        return err({ _tag: 'CiDispatchAmbiguous', workflowUrl: init.ambiguousWorkflowUrl })
      }
      const message = cause instanceof Error ? cause.message.slice(0, 300) : 'Response read failed'
      return err(apiError(0, message.replaceAll(this.token, '[redacted]')))
    }
    if (bytes.byteLength > MAX_RESPONSE_BYTES) {
      if (init?.ambiguousWorkflowUrl) {
        return err({ _tag: 'CiDispatchAmbiguous', workflowUrl: init.ambiguousWorkflowUrl })
      }
      return err(apiError(0, 'GitHub response exceeds the size limit'))
    }
    if (bytes.byteLength === 0) return ok(undefined as T)
    try {
      return ok(JSON.parse(Buffer.from(bytes).toString('utf8')) as T)
    } catch {
      if (init?.ambiguousWorkflowUrl) {
        return err({ _tag: 'CiDispatchAmbiguous', workflowUrl: init.ambiguousWorkflowUrl })
      }
      return err(apiError(0, 'GitHub returned malformed JSON'))
    }
  }

  verifyAuthentication(): ResultAsync<void, CiError> {
    return this.apiRequest<{ login?: string }>('/user')
      .andThen((user) =>
        user.login
          ? ok(undefined)
          : err(apiError(0, 'GitHub returned invalid authenticated-user metadata')),
      )
      .mapErr((error) =>
        error._tag === 'CiApiError' && (error.status === 401 || error.status === 403)
          ? { ...error, authenticationRejected: true as const }
          : error,
      )
  }

  listWorkflows(): ResultAsync<GitHubWorkflow[], CiError> {
    return new ResultAsync(
      (async (): Promise<Result<GitHubWorkflow[], CiError>> => {
        const workflows: GitHubWorkflow[] = []
        for (let page = 1; page <= MAX_PAGES; page += 1) {
          const result = await this.request<{ total_count?: number; workflows?: RawWorkflow[] }>(
            `/actions/workflows?per_page=100&page=${page}`,
          )
          if (result.isErr()) return err(result.error)
          const entries = result.value.workflows ?? []
          workflows.push(
            ...entries.flatMap((workflow) =>
              typeof workflow.id === 'number' && workflow.path
                ? [
                    {
                      id: workflow.id,
                      name: workflow.name || workflow.path,
                      path: workflow.path,
                      state: workflow.state ?? 'unknown',
                      htmlUrl: workflow.html_url ?? '',
                    },
                  ]
                : [],
            ),
          )
          if (entries.length < 100 || workflows.length >= (result.value.total_count ?? 0)) break
        }
        return ok(workflows)
      })(),
    )
  }

  getRepository(): ResultAsync<GitHubRepositoryInfo, CiError> {
    return this.request<{ full_name?: string; default_branch?: string }>('').andThen((raw) =>
      raw.full_name && raw.default_branch
        ? ok({ fullName: raw.full_name, defaultBranch: raw.default_branch })
        : err(apiError(0, 'GitHub returned invalid repository metadata')),
    )
  }

  getWorkflowFile(path: string, ref: string): ResultAsync<GitHubWorkflowFile, CiError> {
    return this.request<RawWorkflowFile>(
      `/contents/${path}?ref=${encodeURIComponent(ref)}`,
    ).andThen((raw) => {
      if (raw.type !== undefined && raw.type !== 'file') {
        return err(apiError(0, 'Workflow path is not a file'))
      }
      if (!raw.sha || raw.encoding !== 'base64' || typeof raw.content !== 'string') {
        return err(apiError(0, 'GitHub returned an invalid workflow file'))
      }
      try {
        return ok({
          sha: raw.sha,
          content: Buffer.from(raw.content.replace(/\s/g, ''), 'base64').toString('utf8'),
        })
      } catch {
        return err(apiError(0, 'GitHub returned invalid workflow content'))
      }
    })
  }

  listBranches(): ResultAsync<GitHubRefResult[], CiError> {
    return this.listRefs('/branches')
  }

  listTags(): ResultAsync<GitHubRefResult[], CiError> {
    return this.listRefs('/tags')
  }

  getExactRef(kind: 'branch' | 'tag', name: string): ResultAsync<GitHubRefResult | null, CiError> {
    const namespace = kind === 'branch' ? 'heads' : 'tags'
    return this.request<{ ref?: string; object?: { sha?: string } }>(
      `/git/ref/${namespace}/${encodeURIComponent(name)}`,
    )
      .andThen((result) =>
        result.object?.sha
          ? ok({ name, commitSha: result.object.sha })
          : err(apiError(0, 'GitHub returned an invalid ref')),
      )
      .orElse((error) =>
        error._tag === 'CiApiError' && error.status === 404 ? ok(null) : err(error),
      )
  }

  private listRefs(path: '/branches' | '/tags'): ResultAsync<GitHubRefResult[], CiError> {
    return new ResultAsync(
      (async (): Promise<Result<GitHubRefResult[], CiError>> => {
        const refs: GitHubRefResult[] = []
        for (let page = 1; page <= MAX_PAGES; page += 1) {
          const result = await this.request<{ name?: string; commit?: { sha?: string } }[]>(
            `${path}?per_page=100&page=${page}`,
          )
          if (result.isErr()) return err(result.error)
          refs.push(
            ...result.value.flatMap((entry) =>
              entry.name && entry.commit?.sha
                ? [{ name: entry.name, commitSha: entry.commit.sha }]
                : [],
            ),
          )
          if (result.value.length < 100) break
        }
        return ok(refs)
      })(),
    )
  }

  listEnvironments(): ResultAsync<GitHubEnvironment[], CiError> {
    return new ResultAsync(
      (async (): Promise<Result<GitHubEnvironment[], CiError>> => {
        const environments: GitHubEnvironment[] = []
        for (let page = 1; page <= MAX_PAGES; page += 1) {
          const result = await this.request<{
            total_count?: number
            environments?: Array<{ name?: string }>
          }>(`/environments?per_page=100&page=${page}`)
          if (result.isErr()) return err(result.error)
          const entries = result.value.environments ?? []
          environments.push(
            ...entries.flatMap((environment) =>
              environment.name ? [{ name: environment.name }] : [],
            ),
          )
          if (entries.length < 100 || environments.length >= (result.value.total_count ?? 0)) break
        }
        return ok(environments)
      })(),
    )
  }

  listWorkflowRuns(workflowId: number, ref?: string): ResultAsync<GitHubWorkflowRun[], CiError> {
    return this.listWorkflowRunsPage(workflowId, ref).map((result) => result.runs)
  }

  listWorkflowRunsPage(
    workflowId: number,
    ref?: string,
  ): ResultAsync<{ runs: GitHubWorkflowRun[]; totalCount: number }, CiError> {
    const query = new URLSearchParams({ per_page: '100', page: '1' })
    if (ref) query.set('branch', ref)
    return this.request<{ total_count?: number; workflow_runs?: GitHubWorkflowRun[] }>(
      `/actions/workflows/${workflowId}/runs?${query.toString()}`,
    ).map((result) => ({
      runs: result.workflow_runs ?? [],
      totalCount: result.total_count ?? result.workflow_runs?.length ?? 0,
    }))
  }

  listRepositoryRuns(
    ref?: string,
    options?: GitHubRepositoryRunsOptions,
  ): ResultAsync<{ runs: GitHubWorkflowRun[]; totalCount: number }, CiError> {
    return new ResultAsync(
      (async (): Promise<Result<{ runs: GitHubWorkflowRun[]; totalCount: number }, CiError>> => {
        const runs: GitHubWorkflowRun[] = []
        let totalCount = 0
        const requestedMaxPages = options?.maxPages ?? MAX_PAGES
        const maxPages = Number.isFinite(requestedMaxPages)
          ? Math.min(MAX_PAGES, Math.max(1, Math.floor(requestedMaxPages)))
          : MAX_PAGES
        for (let page = 1; page <= maxPages; page += 1) {
          const query = new URLSearchParams({ per_page: '100', page: String(page) })
          if (ref) query.set('branch', ref)
          const result = await this.request<{
            total_count?: number
            workflow_runs?: GitHubWorkflowRun[]
          }>(`/actions/runs?${query.toString()}`)
          if (result.isErr()) return err(result.error)
          const entries = result.value.workflow_runs ?? []
          totalCount = result.value.total_count ?? entries.length
          runs.push(...entries)
          if (entries.length < 100 || runs.length >= totalCount || options?.stopWhen?.(runs)) break
        }
        return ok({ runs, totalCount })
      })(),
    )
  }

  getRun(runId: string): ResultAsync<GitHubWorkflowRun, CiError> {
    return this.request<GitHubWorkflowRun>(`/actions/runs/${encodeURIComponent(runId)}`)
  }

  dispatchWorkflow(
    workflowId: number,
    refName: string,
    inputs: Record<string, string | boolean>,
  ): ResultAsync<GitHubDispatchResult, CiError> {
    const workflowUrl = `https://github.com/${this.repositoryPath.slice('/repos/'.length)}/actions`
    return this.request<{
      workflow_run_id?: number
      run_url?: string
      html_url?: string
    }>(`/actions/workflows/${workflowId}/dispatches`, {
      method: 'POST',
      body: { ref: refName, inputs },
      ambiguousWorkflowUrl: workflowUrl,
    }).andThen((response) => {
      if (
        typeof response?.workflow_run_id !== 'number' ||
        !response.run_url ||
        !response.html_url
      ) {
        return err<GitHubDispatchResult, CiError>({
          _tag: 'CiDispatchAmbiguous',
          workflowUrl,
        })
      }
      return ok({
        runId: String(response.workflow_run_id),
        apiUrl: response.run_url,
        webUrl: response.html_url,
      })
    })
  }
}
