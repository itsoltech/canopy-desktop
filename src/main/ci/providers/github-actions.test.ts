import { errAsync, okAsync } from 'neverthrow'
import { describe, expect, it, vi } from 'vitest'
import type { GitHubActionsCiConfig } from '../types'
import type { GitHubActionsClient } from '../github-actions/client'
import { discoverGitHubWorkflows, GitHubActionsAdapter } from './github-actions'

const CONFIG: GitHubActionsCiConfig = {
  provider: 'github-actions',
  baseUrl: 'https://github.com',
  repository: 'itsoltech/canopy-desktop',
  workflows: [{ path: '.github/workflows/release.yml', label: 'Release' }],
}

function fakeClient(overrides: Partial<GitHubActionsClient> = {}): GitHubActionsClient {
  return {
    verifyAuthentication: vi.fn(() => okAsync(undefined)),
    listWorkflows: vi.fn(() =>
      okAsync([
        {
          id: 42,
          name: 'Release',
          path: '.github/workflows/release.yml',
          state: 'active',
          htmlUrl: 'workflow-url',
        },
      ]),
    ),
    listBranches: vi.fn(() => okAsync([{ name: 'next', commitSha: 'commit-sha' }])),
    listTags: vi.fn(() => okAsync([])),
    getExactRef: vi.fn((kind: 'branch' | 'tag', name: string) =>
      okAsync(kind === 'branch' ? { name, commitSha: 'commit-sha' } : null),
    ),
    getWorkflowFile: vi.fn(() =>
      okAsync({
        sha: 'blob-sha',
        content:
          'on:\n  workflow_dispatch:\n    inputs:\n      dry_run:\n        type: boolean\n        default: true\n',
      }),
    ),
    listEnvironments: vi.fn(() => okAsync([])),
    listWorkflowRuns: vi.fn(() => okAsync([])),
    listWorkflowRunsPage: vi.fn(() => okAsync({ runs: [], totalCount: 0 })),
    listRepositoryRuns: vi.fn(() => okAsync({ runs: [], totalCount: 0 })),
    getRun: vi.fn(() => okAsync({ id: 1, status: 'queued' })),
    dispatchWorkflow: vi.fn(() => okAsync({ runId: '123', apiUrl: 'api-run', webUrl: 'web-run' })),
    ...overrides,
  } as unknown as GitHubActionsClient
}

describe('GitHubActionsAdapter', () => {
  it('rejects a run owned by a workflow outside the repository configuration', async () => {
    const client = fakeClient({
      getRun: vi.fn(() =>
        okAsync({
          id: 99,
          status: 'completed',
          conclusion: 'success',
          path: '.github/workflows/foreign.yml@refs/heads/main',
        }),
      ),
    })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.run('99')

    expect(result.isErr()).toBe(true)
  })

  it('rejects public workflow data when GitHub rejects the stored identity', async () => {
    const client = fakeClient()
    vi.mocked(client.verifyAuthentication).mockReturnValue(
      errAsync({
        _tag: 'CiApiError',
        status: 401,
        message: 'Bad credentials',
        provider: 'github-actions',
        authenticationRejected: true,
      }),
    )
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.status({ name: 'next', kind: 'branch' })

    expect(result.isErr()).toBe(true)
    expect(client.listWorkflows).not.toHaveBeenCalled()
  })

  it('never dispatches when the authenticated-user probe is rejected', async () => {
    const client = fakeClient()
    vi.mocked(client.verifyAuthentication).mockReturnValue(
      errAsync({
        _tag: 'CiApiError',
        status: 403,
        message: 'Forbidden',
        provider: 'github-actions',
        authenticationRejected: true,
      }),
    )
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.trigger({
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' },
      schemaRevision: 'blob-sha',
      inputs: { dry_run: true },
    })

    expect(result.isErr()).toBe(true)
    expect(client.dispatchWorkflow).not.toHaveBeenCalled()
  })

  it('discovers dispatchable workflows from the repository default branch with per-file errors', async () => {
    const client = fakeClient({
      listWorkflows: vi.fn(() =>
        okAsync([
          {
            id: 42,
            name: 'Release',
            path: '.github/workflows/release.yml',
            state: 'active',
            htmlUrl: 'release-url',
          },
          {
            id: 43,
            name: 'Checks',
            path: '.github/workflows/checks.yml',
            state: 'active',
            htmlUrl: 'checks-url',
          },
        ]),
      ),
      getWorkflowFile: vi.fn((path: string) =>
        okAsync({
          sha: 'sha',
          content: path.includes('release') ? 'on: workflow_dispatch\n' : 'on: push\n',
        }),
      ),
    })

    const result = await discoverGitHubWorkflows(client, 'next')

    expect(result.isOk() && result.value).toEqual([
      {
        id: '42',
        path: '.github/workflows/release.yml',
        name: 'Release',
        webUrl: 'release-url',
        available: true,
      },
      {
        id: '43',
        path: '.github/workflows/checks.yml',
        name: 'Checks',
        webUrl: 'checks-url',
        available: false,
        error: 'workflow does not declare workflow_dispatch',
      },
    ])
  })

  it('loads the workflow schema from the selected typed ref and returns its blob revision', async () => {
    const client = fakeClient()
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.parameters('.github/workflows/release.yml', {
      name: 'next',
      kind: 'branch',
    })

    expect(result.isOk() && result.value.schemaRevision).toBe('blob-sha')
    expect(result.isOk() && result.value.parameters[0]).toMatchObject({
      name: 'dry_run',
      kind: 'checkbox',
      valueType: 'boolean',
      defaultValue: 'true',
    })
    expect(client.getWorkflowFile).toHaveBeenCalledWith('.github/workflows/release.yml', 'next')
  })

  it('resolves an exact configured ref without relying on the bounded picker list', async () => {
    const client = fakeClient()
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.exactRef('.github/workflows/release.yml', 'beyond-page-five')

    expect(result.isOk() && result.value).toEqual({
      name: 'beyond-page-five',
      kind: 'branch',
      commitSha: 'commit-sha',
    })
    expect(client.listBranches).not.toHaveBeenCalled()
    expect(client.listTags).not.toHaveBeenCalled()
  })

  it('rejects a branch/tag name collision before loading or dispatching', async () => {
    const client = fakeClient({
      getExactRef: vi.fn((_kind: 'branch' | 'tag', name: string) =>
        okAsync({ name, commitSha: 'sha' }),
      ),
    })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.trigger({
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' },
      schemaRevision: 'blob-sha',
      inputs: { dry_run: true },
    })

    expect(result.isErr()).toBe(true)
    expect(client.dispatchWorkflow).not.toHaveBeenCalled()
  })

  it('rejects schema drift before dispatch and dispatches a matching typed payload once', async () => {
    const client = fakeClient()
    const adapter = new GitHubActionsAdapter(CONFIG, client)
    const stale = await adapter.trigger({
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' },
      schemaRevision: 'old-sha',
      inputs: { dry_run: true },
    })
    expect(stale.isErr() && stale.error._tag).toBe('CiWorkflowSchemaChanged')
    expect(client.dispatchWorkflow).not.toHaveBeenCalled()

    const valid = await adapter.trigger({
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' },
      schemaRevision: 'blob-sha',
      inputs: { dry_run: true },
    })
    expect(valid.isOk() && valid.value.runId).toBe('123')
    expect(client.dispatchWorkflow).toHaveBeenCalledOnce()
    expect(client.dispatchWorkflow).toHaveBeenCalledWith(42, 'next', { dry_run: true })
  })

  it('rejects a ref that moved after native confirmation', async () => {
    const client = fakeClient()
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.trigger({
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch', commitSha: 'confirmed-sha' },
      schemaRevision: 'blob-sha',
      inputs: { dry_run: true },
    })

    expect(result.isErr() && result.error._tag).toBe('CiRefChanged')
    expect(client.dispatchWorkflow).not.toHaveBeenCalled()
  })

  it('queries activity once for the repository instead of once per configured workflow', async () => {
    const config: GitHubActionsCiConfig = {
      ...CONFIG,
      workflows: [...CONFIG.workflows, { path: '.github/workflows/tests.yml', label: 'Tests' }],
    }
    const listRepositoryRuns = vi.fn(() => okAsync({ runs: [], totalCount: 0 }))
    const client = fakeClient({
      listWorkflows: vi.fn(() =>
        okAsync([
          {
            id: 42,
            name: 'Release',
            path: '.github/workflows/release.yml',
            state: 'active',
            htmlUrl: '',
          },
          {
            id: 43,
            name: 'Tests',
            path: '.github/workflows/tests.yml',
            state: 'active',
            htmlUrl: '',
          },
        ]),
      ),
      listRepositoryRuns,
    })
    const adapter = new GitHubActionsAdapter(config, client)

    const result = await adapter.activity()

    expect(result.isOk()).toBe(true)
    expect(listRepositoryRuns).toHaveBeenCalledTimes(1)
    expect(listRepositoryRuns).toHaveBeenCalledWith(undefined)
  })

  it('narrows activity to a branch in the QUERY, not the response', async () => {
    // `recent` is sliced to the ten newest across every configured workflow, so a
    // response-side filter would hide a branch whose last run is older than that.
    const listRepositoryRuns = vi.fn(() => okAsync({ runs: [], totalCount: 0 }))
    const client = fakeClient({ listRepositoryRuns })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.activity('feat/x')

    expect(result.isOk()).toBe(true)
    expect(listRepositoryRuns).toHaveBeenCalledWith('feat/x')
  })

  it('marks activity as partial when configured workflow runs may be outside the snapshot', async () => {
    const client = fakeClient({
      listRepositoryRuns: vi.fn(() => okAsync({ runs: [], totalCount: 101 })),
    })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.activity()

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value.partialErrors).toEqual([
      'Older configured workflow runs may be outside the bounded history',
    ])
  })

  it('marks truncated activity partial even when a configured workflow is present', async () => {
    const client = fakeClient({
      listRepositoryRuns: vi.fn(() =>
        okAsync({
          runs: [
            {
              id: 10,
              run_number: 7,
              status: 'in_progress',
              conclusion: null,
              path: '.github/workflows/release.yml@refs/heads/next',
              head_branch: 'next',
            },
          ],
          totalCount: 501,
        }),
      ),
    })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.activity('next')

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value.running).toHaveLength(1)
    expect(result.value.recent).toHaveLength(0)
    expect(result.value.partialErrors).toEqual([
      'Older configured workflow runs may be outside the bounded history',
    ])
  })

  it('does not mark truncated activity partial when the ten recent entries are complete', async () => {
    const client = fakeClient({
      listRepositoryRuns: vi.fn(() =>
        okAsync({
          runs: Array.from({ length: 10 }, (_value, index) => ({
            id: index + 1,
            run_number: index + 1,
            status: 'completed',
            conclusion: 'success',
            path: '.github/workflows/release.yml@refs/heads/next',
            head_branch: 'next',
            created_at: new Date(2026, 0, 10 - index).toISOString(),
          })),
          totalCount: 501,
        }),
      ),
    })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.activity('next')

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value.recent).toHaveLength(10)
    expect(result.value.partialErrors).toBeUndefined()
  })

  it('does not report no run when a configured workflow may be outside the snapshot', async () => {
    const client = fakeClient({
      listRepositoryRuns: vi.fn(() => okAsync({ runs: [], totalCount: 501 })),
    })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.status({ name: 'next', kind: 'branch' })

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value[0]).toMatchObject({
      jobId: '.github/workflows/release.yml',
      run: null,
      error: 'Older runs for Release are outside the bounded history',
    })
    expect(client.listRepositoryRuns).toHaveBeenCalledTimes(1)
    expect(client.listWorkflowRuns).not.toHaveBeenCalled()
  })

  it('keeps status polling request count constant at the configured workflow cap', async () => {
    const workflows = Array.from({ length: 50 }, (_, index) => ({
      path: `.github/workflows/workflow-${index}.yml`,
      label: `Workflow ${index}`,
    }))
    const client = fakeClient({
      listWorkflows: vi.fn(() =>
        okAsync(
          workflows.map((workflow, index) => ({
            id: index + 1,
            name: workflow.label,
            path: workflow.path,
            state: 'active',
            htmlUrl: '',
          })),
        ),
      ),
    })
    const adapter = new GitHubActionsAdapter({ ...CONFIG, workflows }, client)

    const result = await adapter.status({ name: 'next', kind: 'branch' })

    expect(result.isOk()).toBe(true)
    expect(client.listRepositoryRuns).toHaveBeenCalledTimes(1)
    expect(client.listWorkflowRuns).not.toHaveBeenCalled()
  })

  it('preserves configured workflow labels and excludes foreign runs from status and activity', async () => {
    const releaseRun = {
      id: 10,
      run_number: 7,
      status: 'completed',
      conclusion: 'success',
      path: '.github/workflows/release.yml@refs/heads/next',
      head_branch: 'next',
    }
    const client = fakeClient({
      listRepositoryRuns: vi.fn(() =>
        okAsync({
          runs: [
            releaseRun,
            {
              ...releaseRun,
              id: 11,
              path: '.github/workflows/foreign.yml@refs/heads/next',
            },
          ],
          totalCount: 2,
        }),
      ),
    })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const status = await adapter.status({ name: 'next', kind: 'branch' })
    const activity = await adapter.activity('next')

    expect(status.isOk() && status.value[0]?.run).toMatchObject({
      runId: '10',
      jobLabel: 'Release',
    })
    expect(activity.isOk() && activity.value.recent).toHaveLength(1)
    expect(activity.isOk() && activity.value.recent[0]).toMatchObject({
      runId: '10',
      jobLabel: 'Release',
    })
  })
})
