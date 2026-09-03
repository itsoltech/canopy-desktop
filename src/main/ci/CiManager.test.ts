import { describe, expect, it, vi, beforeEach } from 'vitest'
import { ok, err, okAsync, errAsync, ResultAsync, type Result } from 'neverthrow'
import { CiManager } from './CiManager'
import type { RepoConfigManager } from '../taskTracker/RepoConfigManager'
import type { KeychainTokenStore } from '../taskTracker/KeychainTokenStore'
import type { CiActivityBuild, GitHubActionsCiConfig, TeamCityCiConfig } from './types'
import type { CiError } from './errors'
import type { GitHubActionsClient } from './github-actions/client'
import { withCiDegradedCauses } from './degraded'

// The network layer is mocked — these tests pin the SECURITY-relevant glue: the
// configured-build-type allowlist, the token gate, config validation at read time
// and the load→save round-trip of saveConfig.
vi.mock('./teamcity', () => ({
  fetchActivity: vi.fn(() => okAsync({ running: [], queued: [], recent: [] })),
  fetchBranches: vi.fn(() => okAsync(['master'])),
  fetchBuild: vi.fn(() => okAsync({})),
  fetchBuildForBranch: vi.fn(() => okAsync(null)),
  fetchBuildTypes: vi.fn(() => okAsync([])),
  fetchPromptParameters: vi.fn(() => okAsync([])),
  isTeamCityLocatorSafeRef: vi.fn((ref: string) => !/[(),:]/u.test(ref)),
  triggerBuild: vi.fn(() => okAsync({ buildId: 1, webUrl: 'https://tc/1', branchName: 'next' })),
}))

import {
  triggerBuild,
  fetchActivity,
  fetchBuild,
  fetchBuildForBranch,
  fetchBuildTypes,
} from './teamcity'

const VALID_CI = {
  provider: 'teamcity',
  baseUrl: 'https://tc.example.com',
  buildTypes: [{ id: 'Gakko_Build', label: 'Build' }],
} satisfies TeamCityCiConfig

const GITHUB_CI = {
  provider: 'github-actions',
  baseUrl: 'https://github.com',
  repository: 'itsoltech/canopy-desktop',
  workflows: [{ path: '.github/workflows/release.yml', label: 'Release' }],
} satisfies GitHubActionsCiConfig

function fakes(opts?: {
  ci?: unknown
  token?: string | null
  loadFails?: 'notFound' | 'parse'
  /** Overrides what exists() reports — defaults to "file is there unless notFound". */
  exists?: boolean
  saveFails?: boolean
  remoteUrl?: string
  remoteUrlResolver?: () => ResultAsync<string, unknown>
  githubClient?: GitHubActionsClient
  authState?: 'valid' | 'invalid' | 'unknown'
  authCheckedAt?: string
  teamCityApproved?: boolean
  teamCityOriginGate?: (baseUrl: string) => ResultAsync<{ allowPrivate: boolean }, CiError>
}): {
  repoConfigManager: RepoConfigManager
  tokenStore: KeychainTokenStore
  manager: CiManager
} {
  const config = { version: 1, trackers: [], projectOverrides: {}, filters: {}, ci: opts?.ci }
  const repoConfigManager = {
    load: vi.fn(() =>
      opts?.loadFails === 'notFound'
        ? errAsync({ _tag: 'ConfigNotFound', repoRoot: 'r' })
        : opts?.loadFails === 'parse'
          ? errAsync({ _tag: 'ConfigParseError', repoRoot: 'r', reason: 'bad JSON' })
          : okAsync(structuredClone(config)),
    ),
    exists: vi.fn(async () => opts?.exists ?? opts?.loadFails !== 'notFound'),
    init: vi.fn(() => okAsync({ version: 1, trackers: [], projectOverrides: {}, filters: {} })),
    save: vi.fn(() =>
      opts?.saveFails
        ? errAsync({ _tag: 'ConfigWriteError', repoRoot: 'r', reason: 'EACCES: permission denied' })
        : okAsync(undefined),
    ),
  } as unknown as RepoConfigManager
  const tokenStore = {
    resolveCredentialsResult: vi.fn(() =>
      opts?.token === null
        ? err({ _tag: 'CredentialNotFound' as const })
        : ok({ token: opts?.token ?? 'tok', username: undefined }),
    ),
    resolveApprovedCredentialsResult: vi.fn(() =>
      opts?.token === null
        ? err({ _tag: 'CredentialNotFound' as const })
        : opts?.teamCityApproved === false
          ? err({
              _tag: 'CredentialApprovalRequired' as const,
              bindingKey: 'ci:teamcity:repo-config:test',
            })
          : ok({
              token: opts?.token ?? 'tok',
              username: undefined,
              credentialId: 'cred-1',
            }),
    ),
    isCredentialsBindingApproved: vi.fn(() => opts?.teamCityApproved !== false),
    prepareCredentialsBindingsApproval: vi.fn(() =>
      ok({
        credentialId: 'cred-1',
        revision: 'revision-1',
        approvalRequired: opts?.teamCityApproved === false,
      }),
    ),
    approveCredentialsBinding: vi.fn(() => ok(undefined)),
    approveCredentialsBindings: vi.fn(() => ok(undefined)),
    recordResult: vi.fn(),
    setCredentials: vi.fn(() => ok({})),
    getCredentials: vi.fn(() =>
      opts?.token === null
        ? null
        : { token: opts?.token ?? 'tok', username: undefined, credentialId: 'cred-1' },
    ),
    registry: {
      list: vi.fn(() => [
        {
          id: 'cred-1',
          authenticationState: opts?.authState ?? 'unknown',
          authenticationCheckedAt: opts?.authCheckedAt,
        },
      ]),
    },
  } as unknown as KeychainTokenStore
  return {
    repoConfigManager,
    tokenStore,
    manager: new CiManager(
      repoConfigManager,
      tokenStore,
      opts?.remoteUrlResolver ??
        (() => okAsync(opts?.remoteUrl ?? 'git@github.com:itsoltech/canopy-desktop.git')),
      opts?.githubClient ? () => opts.githubClient as GitHubActionsClient : undefined,
      opts?.teamCityOriginGate ?? (() => okAsync({ allowPrivate: false })),
    ),
  }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('loadConfig', () => {
  it('reports a missing ci block as CiNotConfigured', async () => {
    const { manager } = fakes({ ci: undefined })
    const result = await manager.loadConfig('r')
    expect(result.isErr() && result.error._tag).toBe('CiNotConfigured')
  })

  it('keeps a present-but-malformed ci block distinct from "not configured"', async () => {
    // The dialogs that surface this error only open BECAUSE the block exists —
    // "No CI configured" there would send the user to set up what they have.
    for (const ci of ['garbage', { provider: 'jenkins' }]) {
      const { manager } = fakes({ ci })
      const result = await manager.loadConfig('r')
      expect(result.isErr() && result.error._tag).toBe('CiConfigInvalid')
      // scope 'block': only the ci block is wrong — re-saving replaces it.
      expect(result.isErr() && result.error._tag === 'CiConfigInvalid' && result.error.scope).toBe(
        'block',
      )
    }
  })

  it('names the invalid ids when every entry in the block is a typo', async () => {
    // A bulk rename typos every id — the generic "unrecognized shape" would
    // steer the user at a Save that deletes them with the names never shown.
    const ci = {
      provider: 'teamcity',
      baseUrl: 'https://tc.example.com',
      buildTypes: [{ id: 'Gakko-Build' }],
    }
    const { manager } = fakes({ ci })
    const result = await manager.loadConfig('r')
    expect(
      result.isErr() && result.error._tag === 'CiConfigInvalid' && result.error.reason,
    ).toContain('Gakko-Build')
  })

  it('degrades a missing config file to CiNotConfigured', async () => {
    const { manager } = fakes({ loadFails: 'notFound' })
    const result = await manager.loadConfig('r')
    expect(result.isErr() && result.error._tag).toBe('CiNotConfigured')
  })

  it('keeps a config parse failure distinct, carrying the reason', async () => {
    const { manager } = fakes({ loadFails: 'parse' })
    const result = await manager.loadConfig('r')
    expect(result.isErr() && result.error._tag).toBe('CiConfigInvalid')
    expect(
      result.isErr() && result.error._tag === 'CiConfigInvalid' && result.error.reason,
    ).toContain('bad JSON')
    // scope 'file': the whole file cannot be used — the message must not blame
    // the ci block, and saveConfig refuses in this state.
    expect(result.isErr() && result.error._tag === 'CiConfigInvalid' && result.error.scope).toBe(
      'file',
    )
  })

  it('returns the validated config', async () => {
    const { manager } = fakes({ ci: VALID_CI })
    const result = await manager.loadConfig('r')
    expect(result.isOk() && result.value.baseUrl).toBe('https://tc.example.com')
  })
})

describe('TeamCity repository credential approval', () => {
  it('reports a stored server token as requiring approval for an unapproved repository config', () => {
    const { manager } = fakes({ ci: VALID_CI, teamCityApproved: false })

    expect(manager.credentialStatusForConfig('C:/repo', VALID_CI)).toMatchObject({
      hasToken: true,
      approvalRequired: true,
    })
  })

  it('does not use the stored token for an unapproved repository config', async () => {
    const { manager } = fakes({ ci: VALID_CI, teamCityApproved: false })

    const result = await manager.statusFor('C:/repo', VALID_CI, 'next')

    expect(result.isErr() && result.error._tag).toBe('CiCredentialApprovalRequired')
    expect(fetchBuildForBranch).not.toHaveBeenCalled()
  })

  it('checks private-origin trust before reading the approved token or using the network', async () => {
    const originGate = vi.fn(() =>
      errAsync<{ allowPrivate: boolean }, CiError>({
        _tag: 'CiPrivateOriginApprovalRequired',
        baseUrl: VALID_CI.baseUrl,
      }),
    )
    const { manager, tokenStore } = fakes({ ci: VALID_CI, teamCityOriginGate: originGate })

    const result = await manager.statusFor('C:/repo', VALID_CI, 'next')

    expect(result.isErr() && result.error._tag).toBe('CiPrivateOriginApprovalRequired')
    expect(originGate).toHaveBeenCalledWith(VALID_CI.baseUrl)
    expect(tokenStore.resolveApprovedCredentialsResult).not.toHaveBeenCalled()
    expect(fetchBuildForBranch).not.toHaveBeenCalled()
  })

  it('binds only the exact config scope after trusted approval', () => {
    const { manager, tokenStore } = fakes({ ci: VALID_CI })
    const prepared = manager.prepareTeamCityConfigApproval('C:/repo', VALID_CI)._unsafeUnwrap()

    const result = manager.approveTeamCityConfig('C:/repo', VALID_CI, prepared)

    expect(result.isOk()).toBe(true)
    expect(tokenStore.approveCredentialsBinding).toHaveBeenCalledOnce()
    expect(tokenStore.approveCredentialsBinding).toHaveBeenCalledWith(
      'teamcity',
      VALID_CI.baseUrl,
      'builds.read',
      expect.stringContaining('ci:teamcity:repo-config:'),
      expect.objectContaining({ credentialId: 'cred-1', revision: 'revision-1' }),
    )
  })
})

describe('credentialStatusForConfig', () => {
  it('reads the exact configured binding without exposing its secret', () => {
    const checkedAt = '2026-08-08T12:00:00.000Z'
    const { manager, tokenStore } = fakes({
      ci: GITHUB_CI,
      authState: 'invalid',
      authCheckedAt: checkedAt,
    })

    expect(manager.credentialStatusForConfig('C:/repo', GITHUB_CI)).toEqual({
      hasToken: true,
      authenticationState: 'invalid',
      authenticationCheckedAt: checkedAt,
    })
    expect(tokenStore.getCredentials).toHaveBeenCalledWith(
      'github-actions',
      'https://github.com/itsoltech/canopy-desktop',
    )
  })

  it('reports a missing configured credential without listing unrelated entries', () => {
    const { manager } = fakes({ ci: VALID_CI, token: null })

    expect(manager.credentialStatusForConfig('C:/repo', VALID_CI)).toEqual({
      hasToken: false,
      authenticationState: 'unknown',
    })
  })
})

describe('the configured-build-type allowlist', () => {
  it('rejects triggering a job that is not in the repo config', async () => {
    const { manager } = fakes({ ci: VALID_CI })
    const result = await manager.trigger('r', 'Other_Job', 'next')
    expect(result.isErr() && result.error._tag).toBe('CiApiError')
    expect(result.isErr() && result.error._tag === 'CiApiError' && result.error.message).toContain(
      'not configured',
    )
    expect(triggerBuild).not.toHaveBeenCalled()
  })

  it('rejects parameter and branch queries for unconfigured jobs', async () => {
    const { manager } = fakes({ ci: VALID_CI })
    for (const result of [
      await manager.promptParameters('r', 'Other_Job'),
      await manager.branches('r', 'Other_Job'),
    ]) {
      expect(result.isErr()).toBe(true)
    }
  })

  it('rejects a build id whose owning build type is outside this repository config', async () => {
    vi.mocked(fetchBuild).mockReturnValueOnce(
      okAsync({
        id: 123,
        number: '123',
        state: 'running',
        status: 'UNKNOWN',
        statusText: undefined,
        percentageComplete: undefined,
        webUrl: 'https://tc/build/123',
        branchName: 'develop',
        queuedAt: undefined,
        startedAt: undefined,
        finishedAt: undefined,
        buildTypeId: 'Other_Project',
      }),
    )
    const { manager } = fakes({ ci: VALID_CI })

    const result = await manager.build('r', VALID_CI.baseUrl, 123)

    expect(result.isErr()).toBe(true)
    expect(result.isErr() && result.error._tag).toBe('CiApiError')
  })

  it('does not poll a build after the repository switches TeamCity servers', async () => {
    const { manager } = fakes({ ci: VALID_CI })

    const result = await manager.build('r', 'https://other-tc.example.com', 123)

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiApiError',
      status: 409,
      message: 'TeamCity server configuration changed while watching the build',
    })
    expect(fetchBuild).not.toHaveBeenCalled()
  })

  it('passes a configured job through with the stored token and properties', async () => {
    const { manager } = fakes({ ci: VALID_CI })
    const props = [{ name: 'Env', value: 'Test' }]
    const result = await manager.trigger('r', 'Gakko_Build', 'next', props)
    expect(result.isOk()).toBe(true)
    expect(triggerBuild).toHaveBeenCalledWith(
      'https://tc.example.com',
      'tok',
      { allowPrivate: false },
      'Gakko_Build',
      'next',
      props,
    )
  })

  it('keeps activity scoped to jobs configured for this repository', async () => {
    const { manager, tokenStore } = fakes({ ci: VALID_CI })
    const build = (id: number, buildTypeId: string): CiActivityBuild => ({
      id,
      number: String(id),
      state: 'finished' as const,
      status: 'SUCCESS',
      statusText: `${buildTypeId} completed`,
      percentageComplete: undefined,
      webUrl: `https://tc.example.com/build/${id}`,
      branchName: 'develop',
      queuedAt: undefined,
      startedAt: undefined,
      finishedAt: undefined,
      buildTypeId,
      buildTypeName: buildTypeId,
    })
    vi.mocked(fetchActivity).mockReturnValue(
      okAsync(
        withCiDegradedCauses(
          {
            running: [build(1, 'Other_Job')],
            queued: [build(2, 'Gakko_Build'), build(3, 'Other_Job')],
            recent: [build(4, 'Other_Job'), build(5, 'Gakko_Build')],
            partialErrors: ['Queued builds unavailable'],
          },
          [{ _tag: 'CiApiError', status: 403, message: 'Queue forbidden' }],
        ),
      ),
    )

    const result = await manager.activity('r')

    expect(fetchActivity).toHaveBeenCalledWith(
      'https://tc.example.com',
      'tok',
      { allowPrivate: false },
      ['Gakko_Build'],
      undefined,
    )
    expect(result._unsafeUnwrap().running).toEqual([])
    expect(result._unsafeUnwrap().queued.map((item) => item.id)).toEqual([2])
    expect(result._unsafeUnwrap().recent.map((item) => item.id)).toEqual([5])
    expect(result._unsafeUnwrap().partialErrors).toEqual(['Queued builds unavailable'])
    expect(tokenStore.recordResult).toHaveBeenCalledWith(
      'teamcity',
      'https://tc.example.com',
      'builds.read',
      403,
      'Queue forbidden',
      expect.objectContaining({
        usedSecret: 'tok',
        bindingKey: expect.stringContaining('ci:teamcity:repo-config:'),
      }),
    )
  })
})

describe('the token gate', () => {
  it('marks an authenticated-user rejection invalid before accepting public workflow data', async () => {
    const githubClient = {
      verifyAuthentication: vi.fn(() =>
        errAsync({
          _tag: 'CiApiError' as const,
          status: 403,
          message: 'Forbidden',
          provider: 'github-actions' as const,
          authenticationRejected: true,
        }),
      ),
      listWorkflows: vi.fn(() => okAsync([])),
    } as unknown as GitHubActionsClient
    const { manager, tokenStore } = fakes({ ci: GITHUB_CI, githubClient })

    const result = await manager.jobsStatus('r', { name: 'next', kind: 'branch' })

    expect(result.isErr()).toBe(true)
    expect(githubClient.listWorkflows).not.toHaveBeenCalled()
    expect(tokenStore.recordResult).toHaveBeenCalledWith(
      'github-actions',
      'https://github.com/itsoltech/canopy-desktop',
      'actions.read',
      403,
      'Forbidden',
      { usedSecret: 'tok', authenticationRejected: true },
    )
  })

  it('rejects a candidate token when GitHub does not authenticate its identity', async () => {
    const githubClient = {
      verifyAuthentication: vi.fn(() =>
        errAsync({
          _tag: 'CiApiError' as const,
          status: 401,
          message: 'Bad credentials',
          provider: 'github-actions' as const,
          authenticationRejected: true,
        }),
      ),
      getRepository: vi.fn(() =>
        okAsync({ fullName: 'itsoltech/canopy-desktop', defaultBranch: 'next' }),
      ),
    } as unknown as GitHubActionsClient
    const { manager } = fakes({ githubClient })

    const result = await manager.testGitHubConnection('r', 'candidate-token')

    expect(result.isErr()).toBe(true)
    expect(githubClient.getRepository).not.toHaveBeenCalled()
  })

  it('rejects a GitHub repository mismatch before reading the host-wide token', async () => {
    const { manager, tokenStore } = fakes({
      ci: GITHUB_CI,
      remoteUrl: 'git@github.com:someone/other.git',
    })

    const result = await manager.jobsStatus('r', { name: 'next', kind: 'branch' })

    expect(result.isErr() && result.error._tag).toBe('CiRepositoryMismatch')
    expect(tokenStore.resolveCredentialsResult).not.toHaveBeenCalled()
  })

  it('maps an authorized GitHub repository to its dedicated CI credential key', async () => {
    const { manager, tokenStore } = fakes({ ci: GITHUB_CI, token: null })

    const result = await manager.jobsStatus('r', { name: 'next', kind: 'branch' })

    expect(result.isErr() && result.error._tag).toBe('CiAuthMissing')
    expect(tokenStore.resolveCredentialsResult).toHaveBeenCalledWith(
      'github-actions',
      'https://github.com/itsoltech/canopy-desktop',
      'actions.read',
    )
  })

  it('does not store a GitHub credential for a non-GitHub workspace', async () => {
    const { manager, tokenStore } = fakes({
      remoteUrl: 'https://git.example.com/itsoltech/canopy-desktop.git',
    })

    const result = await manager.saveGitHubCredential('r', 'candidate-token')

    expect(result.isErr() && result.error._tag).toBe('CiRepositoryMismatch')
    expect(tokenStore.setCredentials).not.toHaveBeenCalled()
  })

  it('stores GitHub Actions credentials under the normalized repository key', async () => {
    const { manager, tokenStore } = fakes({})

    const result = await manager.saveGitHubCredential('r', 'candidate-token')

    expect(result.isOk()).toBe(true)
    expect(tokenStore.setCredentials).toHaveBeenCalledWith(
      'github-actions',
      'https://github.com/itsoltech/canopy-desktop',
      'candidate-token',
    )
  })

  it('fails with CiAuthMissing before any network call when no token is stored', async () => {
    const { manager } = fakes({ ci: VALID_CI, token: null })
    const result = await manager.trigger('r', 'Gakko_Build', 'next')
    expect(result.isErr() && result.error._tag).toBe('CiAuthMissing')
    expect(triggerBuild).not.toHaveBeenCalled()
  })

  it('gates listBuildTypes the same way — the one method taking a renderer-supplied URL', async () => {
    const { manager } = fakes({ token: null })
    const result = await manager.listBuildTypes('C:/repo', 'https://tc.example.com')
    expect(result.isErr() && result.error._tag).toBe('CiAuthMissing')
    expect(fetchBuildTypes).not.toHaveBeenCalled()
  })

  it('passes listBuildTypes through with the stored token', async () => {
    const { manager } = fakes({})
    const result = await manager.listBuildTypes('C:/repo', 'https://tc.example.com')
    expect(result.isOk()).toBe(true)
    expect(fetchBuildTypes).toHaveBeenCalledWith('https://tc.example.com', 'tok', {
      allowPrivate: false,
    })
  })

  it('normalizes an existing stored TeamCity token before every API use', async () => {
    const { manager } = fakes({ token: '  tok\r\n' })
    const result = await manager.listBuildTypes('C:/repo', 'https://tc.example.com')
    expect(result.isOk()).toBe(true)
    expect(fetchBuildTypes).toHaveBeenCalledWith('https://tc.example.com', 'tok', {
      allowPrivate: false,
    })
  })

  it('treats an existing whitespace-only TeamCity token as missing', async () => {
    const { manager } = fakes({ token: ' \r\n ' })
    const result = await manager.listBuildTypes('C:/repo', 'https://tc.example.com')
    expect(result.isErr() && result.error._tag).toBe('CiAuthMissing')
    expect(fetchBuildTypes).not.toHaveBeenCalled()
  })
})

describe('GitHub dispatch confirmation', () => {
  function githubClient(
    dispatchWorkflow = vi.fn(() => okAsync({ runId: '1', apiUrl: '', webUrl: 'run-url' })),
  ): GitHubActionsClient {
    return {
      verifyAuthentication: vi.fn(() => okAsync(undefined)),
      listWorkflows: vi.fn(() =>
        okAsync([
          {
            id: 42,
            name: 'Release',
            path: '.github/workflows/release.yml',
            state: 'active',
            htmlUrl: '',
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
            'on:\n  workflow_dispatch:\n    inputs:\n      dry_run:\n        type: boolean\n        required: true\n',
        }),
      ),
      listEnvironments: vi.fn(() => okAsync([])),
      dispatchWorkflow,
    } as unknown as GitHubActionsClient
  }

  const request = {
    jobId: '.github/workflows/release.yml',
    ref: { name: 'next', kind: 'branch' as const },
    schemaRevision: 'blob-sha',
    inputs: { dry_run: true },
  }

  it('dispatches after the in-app confirmation when no native callback is configured', async () => {
    const dispatch = vi.fn(() => okAsync({ runId: '1', apiUrl: '', webUrl: 'run-url' }))
    const { manager } = fakes({ ci: GITHUB_CI, githubClient: githubClient(dispatch) })

    const result = await manager.triggerJob('r', request)

    expect(result.isOk() && result.value.runId).toBe('1')
    expect(dispatch).toHaveBeenCalledOnce()
  })

  it('dispatches once only after the trusted confirmation accepts main-resolved details', async () => {
    const dispatch = vi.fn(() => okAsync({ runId: '1', apiUrl: '', webUrl: 'run-url' }))
    const confirm = vi.fn(async () => true)
    const { manager } = fakes({ ci: GITHUB_CI, githubClient: githubClient(dispatch) })

    const result = await manager.triggerJob('r', request, confirm)

    expect(result.isOk() && result.value.runId).toBe('1')
    expect(confirm).toHaveBeenCalledWith(
      expect.objectContaining({
        repository: 'itsoltech/canopy-desktop',
        workflowPath: '.github/workflows/release.yml',
        ref: expect.objectContaining({ name: 'next', kind: 'branch', commitSha: 'commit-sha' }),
        inputs: { dry_run: true },
      }),
    )
    expect(dispatch).toHaveBeenCalledOnce()
  })

  it('does not dispatch when a configured native confirmation declines', async () => {
    const dispatch = vi.fn(() => okAsync({ runId: '1', apiUrl: '', webUrl: 'run-url' }))
    const confirm = vi.fn(async () => false)
    const { manager } = fakes({ ci: GITHUB_CI, githubClient: githubClient(dispatch) })

    const result = await manager.triggerJob('r', request, confirm)

    expect(result.isErr() && result.error._tag).toBe('CiDispatchCancelled')
    expect(confirm).toHaveBeenCalledOnce()
    expect(dispatch).not.toHaveBeenCalled()
  })

  it('fails closed when a configured native confirmation rejects', async () => {
    const dispatch = vi.fn(() => okAsync({ runId: '1', apiUrl: '', webUrl: 'run-url' }))
    const confirm = vi.fn(async () => {
      throw new Error('window disappeared')
    })
    const { manager } = fakes({ ci: GITHUB_CI, githubClient: githubClient(dispatch) })

    const result = await manager.triggerJob('r', request, confirm)

    expect(result.isErr() && result.error._tag).toBe('CiApiError')
    expect(dispatch).not.toHaveBeenCalled()
  })

  it('validates renderer inputs before showing the trusted confirmation', async () => {
    const dispatch = vi.fn(() => okAsync({ runId: '1', apiUrl: '', webUrl: 'run-url' }))
    const confirm = vi.fn(async () => true)
    const { manager } = fakes({ ci: GITHUB_CI, githubClient: githubClient(dispatch) })

    const result = await manager.triggerJob(
      'r',
      { ...request, inputs: { unexpected: 'value' } },
      confirm,
    )

    expect(result.isErr() && result.error._tag).toBe('CiWorkflowSchemaInvalid')
    expect(confirm).not.toHaveBeenCalled()
    expect(dispatch).not.toHaveBeenCalled()
  })
})

describe('statusFor', () => {
  it('records a clean polled TeamCity read as verified', async () => {
    const { manager, tokenStore } = fakes({ ci: VALID_CI })
    const config = (await manager.loadConfig('r'))._unsafeUnwrap()

    const result = await manager.statusFor('C:/repo', config, 'next')

    expect(result.isOk()).toBe(true)
    expect(tokenStore.recordResult).toHaveBeenCalledWith(
      'teamcity',
      'https://tc.example.com',
      'builds.read',
      200,
      undefined,
      expect.objectContaining({
        usedSecret: 'tok',
        bindingKey: expect.stringContaining('ci:teamcity:repo-config:'),
      }),
    )
  })

  it('records an all-row 401 as an authentication failure instead of a verified success', async () => {
    const ci = {
      ...VALID_CI,
      buildTypes: [
        { id: 'First_Job', label: 'First' },
        { id: 'Second_Job', label: 'Second' },
      ],
    }
    const { manager, tokenStore } = fakes({ ci })
    vi.mocked(fetchBuildForBranch)
      .mockImplementationOnce(() =>
        errAsync({ _tag: 'CiApiError' as const, status: 401, message: 'Unauthorized' }),
      )
      .mockImplementationOnce(() =>
        errAsync({ _tag: 'CiApiError' as const, status: 401, message: 'Unauthorized' }),
      )

    const config = (await manager.loadConfig('r'))._unsafeUnwrap()
    const result = await manager.statusFor('C:/repo', config, 'next')

    expect(result.isOk() && result.value.every((row) => row.error?.includes('401'))).toBe(true)
    expect(tokenStore.recordResult).toHaveBeenCalledWith(
      'teamcity',
      'https://tc.example.com',
      'builds.read',
      401,
      'Unauthorized',
      expect.objectContaining({
        usedSecret: 'tok',
        bindingKey: expect.stringContaining('ci:teamcity:repo-config:'),
      }),
    )
    expect(vi.mocked(tokenStore.recordResult).mock.calls.some((call) => call[3] === 200)).toBe(
      false,
    )
  })

  it('records a PARTIAL 401 as an authentication failure, never as a success', async () => {
    // A token scoped away from ONE build type: requiring every row to fail would
    // leave the causes unattached, and the poll would re-stamp the credential
    // "verified" every 10–45 s while the sidebar shows Unavailable.
    const ci = {
      ...VALID_CI,
      buildTypes: [
        { id: 'Good_Job', label: 'Good' },
        { id: 'Scoped_Away', label: 'Scoped' },
      ],
    }
    const { manager, tokenStore } = fakes({ ci })
    vi.mocked(fetchBuildForBranch).mockImplementation((_url, _tok, _connection, id) =>
      id === 'Scoped_Away'
        ? errAsync({ _tag: 'CiApiError' as const, status: 401, message: 'Unauthorized' })
        : okAsync(null),
    )

    const config = (await manager.loadConfig('r'))._unsafeUnwrap()
    const result = await manager.statusFor('C:/repo', config, 'next')

    expect(result.isOk()).toBe(true)
    const rows = result._unsafeUnwrap()
    expect(rows[0].error).toBeUndefined()
    expect(rows[1].error).toContain('401')
    expect(tokenStore.recordResult).toHaveBeenCalledWith(
      'teamcity',
      'https://tc.example.com',
      'builds.read',
      401,
      'Unauthorized',
      expect.objectContaining({
        usedSecret: 'tok',
        bindingKey: expect.stringContaining('ci:teamcity:repo-config:'),
      }),
    )
    expect(vi.mocked(tokenStore.recordResult).mock.calls.some((call) => call[3] === 200)).toBe(
      false,
    )
  })

  it('rejects locator-unsafe branches before calling TeamCity', async () => {
    const { manager } = fakes({ ci: VALID_CI })
    const config = (await manager.loadConfig('r'))._unsafeUnwrap()

    const result = await manager.statusFor('C:/repo', config, 'feat(ci),v2')

    expect(result.isErr() && result.error).toMatchObject({
      _tag: 'CiApiError',
      message: 'TeamCity branch contains locator-unsafe characters',
    })
    expect(fetchBuildForBranch).not.toHaveBeenCalled()
  })

  it('degrades a failed row to Unavailable while siblings keep their builds', async () => {
    const ci = {
      provider: 'teamcity',
      baseUrl: 'https://tc.example.com',
      buildTypes: [
        { id: 'Good_Job', label: 'Good' },
        { id: 'Dead_Job', label: 'Dead' },
      ],
    }
    const { manager, tokenStore } = fakes({ ci })
    // The survivor must carry a REAL build — with okAsync(null) here, a regression
    // that nulls every sibling's build would produce the exact passing state.
    vi.mocked(fetchBuildForBranch).mockImplementation((_url, _tok, _connection, id) =>
      id === 'Dead_Job'
        ? errAsync({ _tag: 'CiApiError' as const, status: 404, message: 'No build type found' })
        : okAsync({
            id: 7,
            number: '42',
            state: 'finished' as const,
            status: 'SUCCESS' as const,
            statusText: 'Build completed',
            percentageComplete: undefined,
            webUrl: 'https://tc.example.com/build/7',
            branchName: 'next',
            queuedAt: undefined,
            startedAt: undefined,
            finishedAt: undefined,
          }),
    )
    const config = (await manager.loadConfig('r'))._unsafeUnwrap()
    const result = await manager.statusFor('C:/repo', config, 'next')
    expect(result.isOk()).toBe(true)
    const rows = result._unsafeUnwrap()
    expect(rows).toHaveLength(2)
    expect(rows[0].error).toBeUndefined()
    expect(rows[0].build?.number).toBe('42')
    expect(rows[1].error).toContain('404')
    expect(rows[1].build).toBeNull()
    // A deleted build-type id says nothing about the TOKEN: the attached causes
    // carry no 401/403, so neither a success nor a failure may be recorded.
    expect(tokenStore.recordResult).not.toHaveBeenCalled()
  })

  it('uses the provided config without a second config read', async () => {
    const { manager, repoConfigManager } = fakes({ ci: VALID_CI })
    const config = (await manager.loadConfig('r'))._unsafeUnwrap()
    vi.mocked(repoConfigManager.load).mockClear()
    const result = await manager.statusFor('C:/repo', config, 'next')
    expect(result.isOk()).toBe(true)
    expect(repoConfigManager.load).not.toHaveBeenCalled()
    expect(fetchBuildForBranch).toHaveBeenCalledWith(
      'https://tc.example.com',
      'tok',
      { allowPrivate: false },
      'Gakko_Build',
      'next',
    )
  })
})

describe('credential verification for partial activity', () => {
  it('records a structured partial 403 without marking the credential verified', async () => {
    const { manager, tokenStore } = fakes({ ci: VALID_CI })
    vi.mocked(fetchActivity).mockReturnValueOnce(
      okAsync(
        withCiDegradedCauses(
          {
            running: [],
            queued: [],
            recent: [],
            partialErrors: ['Queued builds: forbidden'],
          },
          [{ _tag: 'CiApiError', status: 403, message: 'Forbidden' }],
        ),
      ),
    )

    const result = await manager.runActivity('r')

    expect(result.isOk()).toBe(true)
    expect(tokenStore.recordResult).toHaveBeenCalledWith(
      'teamcity',
      'https://tc.example.com',
      'builds.read',
      403,
      'Forbidden',
      expect.objectContaining({
        usedSecret: 'tok',
        bindingKey: expect.stringContaining('ci:teamcity:repo-config:'),
      }),
    )
    expect(vi.mocked(tokenStore.recordResult).mock.calls.some((call) => call[3] === 200)).toBe(
      false,
    )
  })

  it('does not infer authentication failure from status-like text in a partial 500', async () => {
    const { manager, tokenStore } = fakes({ ci: VALID_CI })
    vi.mocked(fetchActivity).mockReturnValueOnce(
      okAsync(
        withCiDegradedCauses(
          {
            running: [],
            queued: [],
            recent: [],
            partialErrors: ['Recent builds: upstream said HTTP 403 while handling a 500'],
          },
          [{ _tag: 'CiApiError', status: 500, message: 'upstream said HTTP 403' }],
        ),
      ),
    )

    const result = await manager.runActivity('r')

    expect(result.isOk()).toBe(true)
    expect(tokenStore.recordResult).not.toHaveBeenCalled()
  })
})

describe('saveConfig', () => {
  it('rejects a GitHub repository mismatch before writing shared configuration', async () => {
    const { manager, repoConfigManager, tokenStore } = fakes({
      ci: undefined,
      remoteUrl: 'git@github.com:someone/other.git',
    })

    const result = await manager.saveConfig('r', GITHUB_CI)

    expect(result.isErr() && result.error._tag).toBe('CiRepositoryMismatch')
    expect(repoConfigManager.save).not.toHaveBeenCalled()
    expect(tokenStore.resolveCredentialsResult).not.toHaveBeenCalled()
  })

  it('writes the ci block through the normal round-trip', async () => {
    const { manager, repoConfigManager } = fakes({ ci: undefined })
    const ci = { provider: 'teamcity' as const, baseUrl: 'https://tc', buildTypes: [] }
    const result = await manager.saveConfig('r', ci)
    expect(result.isOk()).toBe(true)
    expect(repoConfigManager.save).toHaveBeenCalledWith('r', expect.objectContaining({ ci }))
  })

  it('initializes a missing config file before writing', async () => {
    const { manager, repoConfigManager } = fakes({ loadFails: 'notFound' })
    const result = await manager.saveConfig('r', null)
    expect(result.isOk()).toBe(true)
    expect(repoConfigManager.init).toHaveBeenCalled()
    expect(repoConfigManager.save).toHaveBeenCalledWith(
      'r',
      expect.objectContaining({ ci: undefined }),
    )
  })

  it('refuses to init over a config that exists but cannot be parsed', async () => {
    // init OVERWRITES with defaults — falling back to it on a parse error would
    // delete the repo's trackers, templates and agent config over a typo in the
    // very block this save was about to replace.
    const { manager, repoConfigManager } = fakes({ loadFails: 'parse' })
    const result = await manager.saveConfig('r', null)
    expect(result.isErr()).toBe(true)
    // A local parse failure must NOT come back as a "TeamCity: …" CiApiError.
    expect(result.isErr() && result.error._tag).toBe('CiConfigInvalid')
    expect(result.isErr() && result.error._tag === 'CiConfigInvalid' && result.error.scope).toBe(
      'file',
    )
    expect(repoConfigManager.init).not.toHaveBeenCalled()
    expect(repoConfigManager.save).not.toHaveBeenCalled()
  })

  it('serializes overlapping read-modify-write cycles on the same repo', async () => {
    // Save racing Remove: without the per-repo chain, the second cycle's load can
    // read the pre-write state and its save resurrect what the first just removed.
    const { manager, repoConfigManager } = fakes({ ci: VALID_CI })
    const order: string[] = []
    let release!: () => void
    const gate = new Promise<void>((r) => (release = r))
    let loads = 0
    let saves = 0
    vi.mocked(repoConfigManager.load).mockImplementation((() => {
      loads += 1
      order.push(`load${loads}`)
      return okAsync({ version: 1, trackers: [], projectOverrides: {}, filters: {} })
    }) as never)
    vi.mocked(repoConfigManager.save).mockImplementation((() => {
      saves += 1
      order.push(`save${saves}`)
      return saves === 1 ? ResultAsync.fromSafePromise(gate) : okAsync(undefined)
    }) as never)
    const ci = { provider: 'teamcity' as const, baseUrl: 'https://tc', buildTypes: [] }
    const first = manager.saveConfig('r', ci)
    const second = manager.saveConfig('r', null)
    await new Promise((r) => setTimeout(r, 0))
    // The second cycle must not have started reading while the first still writes.
    expect(order).toEqual(['load1', 'save1'])
    release()
    expect((await first).isOk()).toBe(true)
    expect((await second).isOk()).toBe(true)
    expect(order).toEqual(['load1', 'save1', 'load2', 'save2'])
  })

  it('queues GitHub validation before a later remove can update the same repo', async () => {
    let releaseValidation!: () => void
    const validation = new Promise<Result<string, unknown>>((resolve) => {
      releaseValidation = () =>
        resolve(ok<string, unknown>('git@github.com:itsoltech/canopy-desktop.git'))
    })
    const { manager, repoConfigManager } = fakes({
      ci: undefined,
      remoteUrlResolver: () => new ResultAsync(validation),
    })

    const save = manager.saveConfig('r', GITHUB_CI)
    const remove = manager.saveConfig('r', null)
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(repoConfigManager.save).not.toHaveBeenCalled()
    releaseValidation()
    expect((await save).isOk()).toBe(true)
    expect((await remove).isOk()).toBe(true)
    expect(vi.mocked(repoConfigManager.save).mock.calls.map((call) => call[1].ci)).toEqual([
      GITHUB_CI,
      undefined,
    ])
  })

  it('reports a write failure as a local error, never as TeamCity', async () => {
    // The whole save chain is local filesystem work — an EACCES on a read-only
    // checkout must not send the user to test their TeamCity connection.
    const { manager } = fakes({ ci: VALID_CI, saveFails: true })
    const result = await manager.saveConfig('r', null)
    expect(result.isErr() && result.error._tag).toBe('CiConfigUnwritable')
  })

  it('refuses to init when the file exists but the read failed with the lossy tag', async () => {
    // RepoConfigManager.load maps EVERY readFile rejection to ConfigNotFound — a
    // transient EMFILE/EACCES on an existing file must not read as "absent" and
    // be initialized over. The filesystem decides, not the error tag.
    const { manager, repoConfigManager } = fakes({ loadFails: 'notFound', exists: true })
    const result = await manager.saveConfig('r', null)
    expect(result.isErr()).toBe(true)
    // The lossy tag means EACCES/EMFILE on a file that IS there — not absence,
    // and not TeamCity. The reason must not repeat load()'s "not found" claim.
    expect(result.isErr() && result.error._tag).toBe('CiConfigUnwritable')
    expect(
      result.isErr() && result.error._tag === 'CiConfigUnwritable' && result.error.reason,
    ).not.toContain('not found')
    expect(repoConfigManager.init).not.toHaveBeenCalled()
    expect(repoConfigManager.save).not.toHaveBeenCalled()
  })
})
