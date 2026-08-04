import { okAsync } from 'neverthrow'
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
    getRun: vi.fn(() => okAsync({ id: 1, status: 'queued' })),
    dispatchWorkflow: vi.fn(() => okAsync({ runId: '123', apiUrl: 'api-run', webUrl: 'web-run' })),
    ...overrides,
  } as unknown as GitHubActionsClient
}

describe('GitHubActionsAdapter', () => {
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

  it('queries activity per configured workflow instead of repository-wide history', async () => {
    const config: GitHubActionsCiConfig = {
      ...CONFIG,
      workflows: [...CONFIG.workflows, { path: '.github/workflows/tests.yml', label: 'Tests' }],
    }
    const listWorkflowRunsPage = vi.fn(() => okAsync({ runs: [], totalCount: 0 }))
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
      listWorkflowRunsPage,
    })
    const adapter = new GitHubActionsAdapter(config, client)

    const result = await adapter.activity()

    expect(result.isOk()).toBe(true)
    expect(listWorkflowRunsPage).toHaveBeenCalledTimes(2)
    expect(listWorkflowRunsPage).toHaveBeenNthCalledWith(1, 42)
    expect(listWorkflowRunsPage).toHaveBeenNthCalledWith(2, 43)
  })

  it('marks activity partial when a configured workflow exceeds the bounded page', async () => {
    const client = fakeClient({
      listWorkflowRunsPage: vi.fn(() => okAsync({ runs: [], totalCount: 101 })),
    })
    const adapter = new GitHubActionsAdapter(CONFIG, client)

    const result = await adapter.activity()

    expect(result.isOk() && result.value.partialErrors).toEqual([
      'Release: showing the newest 100 of 101 runs',
    ])
  })
})
