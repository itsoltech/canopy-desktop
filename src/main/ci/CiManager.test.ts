import { describe, expect, it, vi, beforeEach } from 'vitest'
import { ok, okAsync, errAsync, ResultAsync, type Result } from 'neverthrow'
import { CiManager } from './CiManager'
import type { RepoConfigManager } from '../taskTracker/RepoConfigManager'
import type { KeychainTokenStore } from '../taskTracker/KeychainTokenStore'
import type { CiActivityBuild, GitHubActionsCiConfig } from './types'
import type { GitHubActionsClient } from './github-actions/client'

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
  triggerBuild: vi.fn(() => okAsync({ buildId: 1, webUrl: 'https://tc/1', branchName: 'next' })),
}))

import { triggerBuild, fetchActivity, fetchBuildForBranch, fetchBuildTypes } from './teamcity'

const VALID_CI = {
  provider: 'teamcity',
  baseUrl: 'https://tc.example.com',
  buildTypes: [{ id: 'Gakko_Build', label: 'Build' }],
}

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
    getCredentials: vi.fn(() =>
      opts?.token === null ? null : { token: opts?.token ?? 'tok', username: undefined },
    ),
    setCredentials: vi.fn(async () => undefined),
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

  it('passes a configured job through with the stored token and properties', async () => {
    const { manager } = fakes({ ci: VALID_CI })
    const props = [{ name: 'Env', value: 'Test' }]
    const result = await manager.trigger('r', 'Gakko_Build', 'next', props)
    expect(result.isOk()).toBe(true)
    expect(triggerBuild).toHaveBeenCalledWith(
      'https://tc.example.com',
      'tok',
      'Gakko_Build',
      'next',
      props,
    )
  })

  it('keeps activity scoped to jobs configured for this repository', async () => {
    const { manager } = fakes({ ci: VALID_CI })
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
      okAsync({
        running: [build(1, 'Other_Job')],
        queued: [build(2, 'Gakko_Build'), build(3, 'Other_Job')],
        recent: [build(4, 'Other_Job'), build(5, 'Gakko_Build')],
        partialErrors: ['Queued builds unavailable'],
      }),
    )

    const result = await manager.activity('r')

    expect(fetchActivity).toHaveBeenCalledWith('https://tc.example.com', 'tok', ['Gakko_Build'])
    expect(result._unsafeUnwrap().running).toEqual([])
    expect(result._unsafeUnwrap().queued.map((item) => item.id)).toEqual([2])
    expect(result._unsafeUnwrap().recent.map((item) => item.id)).toEqual([5])
    expect(result._unsafeUnwrap().partialErrors).toEqual(['Queued builds unavailable'])
  })
})

describe('the token gate', () => {
  it('rejects a GitHub repository mismatch before reading the host-wide token', async () => {
    const { manager, tokenStore } = fakes({
      ci: GITHUB_CI,
      remoteUrl: 'git@github.com:someone/other.git',
    })

    const result = await manager.jobsStatus('r', { name: 'next', kind: 'branch' })

    expect(result.isErr() && result.error._tag).toBe('CiRepositoryMismatch')
    expect(tokenStore.getCredentials).not.toHaveBeenCalled()
  })

  it('maps an authorized GitHub repository to its dedicated CI credential key', async () => {
    const { manager, tokenStore } = fakes({ ci: GITHUB_CI, token: null })

    const result = await manager.jobsStatus('r', { name: 'next', kind: 'branch' })

    expect(result.isErr() && result.error._tag).toBe('CiAuthMissing')
    expect(tokenStore.getCredentials).toHaveBeenCalledWith(
      'github-actions',
      'https://github.com/itsoltech/canopy-desktop',
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
    const result = await manager.listBuildTypes('https://tc.example.com')
    expect(result.isErr() && result.error._tag).toBe('CiAuthMissing')
    expect(fetchBuildTypes).not.toHaveBeenCalled()
  })

  it('passes listBuildTypes through with the stored token', async () => {
    const { manager } = fakes({})
    const result = await manager.listBuildTypes('https://tc.example.com')
    expect(result.isOk()).toBe(true)
    expect(fetchBuildTypes).toHaveBeenCalledWith('https://tc.example.com', 'tok')
  })

  it('normalizes an existing stored TeamCity token before every API use', async () => {
    const { manager } = fakes({ token: '  tok\r\n' })
    const result = await manager.listBuildTypes('https://tc.example.com')
    expect(result.isOk()).toBe(true)
    expect(fetchBuildTypes).toHaveBeenCalledWith('https://tc.example.com', 'tok')
  })

  it('treats an existing whitespace-only TeamCity token as missing', async () => {
    const { manager } = fakes({ token: ' \r\n ' })
    const result = await manager.listBuildTypes('https://tc.example.com')
    expect(result.isErr() && result.error._tag).toBe('CiAuthMissing')
    expect(fetchBuildTypes).not.toHaveBeenCalled()
  })
})

describe('GitHub dispatch confirmation', () => {
  function githubClient(
    dispatchWorkflow = vi.fn(() => okAsync({ runId: '1', apiUrl: '', webUrl: 'run-url' })),
  ): GitHubActionsClient {
    return {
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

  it('cannot dispatch without a trusted main-process confirmation callback', async () => {
    const dispatch = vi.fn(() => okAsync({ runId: '1', apiUrl: '', webUrl: 'run-url' }))
    const { manager } = fakes({ ci: GITHUB_CI, githubClient: githubClient(dispatch) })

    const result = await manager.triggerJob('r', request)

    expect(result.isErr() && result.error._tag).toBe('CiDispatchCancelled')
    expect(dispatch).not.toHaveBeenCalled()
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
  it('degrades a failed row to Unavailable while siblings keep their builds', async () => {
    const ci = {
      provider: 'teamcity',
      baseUrl: 'https://tc.example.com',
      buildTypes: [
        { id: 'Good_Job', label: 'Good' },
        { id: 'Dead_Job', label: 'Dead' },
      ],
    }
    const { manager } = fakes({ ci })
    // The survivor must carry a REAL build — with okAsync(null) here, a regression
    // that nulls every sibling's build would produce the exact passing state.
    vi.mocked(fetchBuildForBranch).mockImplementation((_url, _tok, id) =>
      id === 'Dead_Job'
        ? errAsync({ _tag: 'CiApiError' as const, status: 404, message: 'No build type found' })
        : okAsync({
            id: 7,
            number: '42',
            state: 'finished' as const,
            status: 'SUCCESS' as const,
            percentageComplete: undefined,
            webUrl: 'https://tc.example.com/build/7',
            branchName: 'next',
          }),
    )
    const config = (await manager.loadConfig('r'))._unsafeUnwrap()
    const result = await manager.statusFor(config, 'next')
    expect(result.isOk()).toBe(true)
    const rows = result._unsafeUnwrap()
    expect(rows).toHaveLength(2)
    expect(rows[0].error).toBeUndefined()
    expect(rows[0].build?.number).toBe('42')
    expect(rows[1].error).toContain('404')
    expect(rows[1].build).toBeNull()
  })

  it('uses the provided config without a second config read', async () => {
    const { manager, repoConfigManager } = fakes({ ci: VALID_CI })
    const config = (await manager.loadConfig('r'))._unsafeUnwrap()
    vi.mocked(repoConfigManager.load).mockClear()
    const result = await manager.statusFor(config, 'next')
    expect(result.isOk()).toBe(true)
    expect(repoConfigManager.load).not.toHaveBeenCalled()
    expect(fetchBuildForBranch).toHaveBeenCalledWith(
      'https://tc.example.com',
      'tok',
      'Gakko_Build',
      'next',
    )
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
    expect(tokenStore.getCredentials).not.toHaveBeenCalled()
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
