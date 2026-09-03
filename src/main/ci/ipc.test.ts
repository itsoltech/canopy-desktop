import { describe, expect, it, vi, beforeEach } from 'vitest'
import { err, errAsync, ok, okAsync } from 'neverthrow'
import { registerCiHandlers } from './ipc'
import type { CiManager } from './CiManager'
import { testConnection as ciTestConnection } from './teamcity'

// Pins the AUTHORIZATION contract of the repo-scoped CI channels: a renderer-
// supplied repoRoot outside the sender's workspaces must be rejected BEFORE any
// CiManager call (no config read, no token use, no write), and on success only
// the RESOLVED path may flow downstream.

vi.mock('./teamcity', () => ({
  isTeamCityLocatorSafeRef: vi.fn((ref: string) => !/[(),:]/u.test(ref)),
  testConnection: vi.fn(() => okAsync(undefined)),
}))

function harness({
  nativeConfirmation = true,
  teamCityConfirmation = true,
  teamCityConfigApprovalRequired = false,
  teamCityDiscoveryApprovalRequired = false,
  privateOriginApprovalRequired = false,
  rotateTeamCityCredentialDuringConfirmation = false,
}: {
  nativeConfirmation?: boolean
  teamCityConfirmation?: boolean
  teamCityConfigApprovalRequired?: boolean
  teamCityDiscoveryApprovalRequired?: boolean
  privateOriginApprovalRequired?: boolean
  rotateTeamCityCredentialDuringConfirmation?: boolean
} = {}): {
  invoke: (channel: string, payload: unknown) => Promise<unknown>
  ciManager: CiManager
  validatePathAccess: ReturnType<typeof vi.fn>
  confirmGitHubDispatch: ReturnType<typeof vi.fn>
  confirmTeamCityAccess: ReturnType<typeof vi.fn>
  teamCityOriginTrust: {
    requiresApproval: ReturnType<typeof vi.fn>
    approve: ReturnType<typeof vi.fn>
    ensureAllowed: ReturnType<typeof vi.fn>
  }
} {
  let currentCredential = { credentialId: 'cred-1', revision: 'revision-1' }
  const handlers = new Map<string, (event: unknown, payload: unknown) => unknown>()
  const ipcMain = {
    handle: (channel: string, listener: (event: unknown, payload: unknown) => unknown) => {
      handlers.set(channel, listener)
    },
  }
  const ciManager = {
    loadConfig: vi.fn(() =>
      okAsync({ provider: 'teamcity', baseUrl: 'https://tc', buildTypes: [] }),
    ),
    credentialStatusForConfig: vi.fn(() => ({
      hasToken: true,
      authenticationState: 'valid',
    })),
    statusFor: vi.fn(() => okAsync([])),
    trigger: vi.fn(() => okAsync({ buildId: 1, webUrl: 'https://tc/1', branchName: 'next' })),
    activity: vi.fn(() => okAsync({ running: [], queued: [], recent: [] })),
    branches: vi.fn(() => okAsync(['master'])),
    promptParameters: vi.fn(() => okAsync([])),
    listBuildTypes: vi.fn(() => okAsync([])),
    saveConfig: vi.fn(() => okAsync(undefined)),
    build: vi.fn(() =>
      okAsync({ id: 1, number: '1', state: 'finished', status: 'SUCCESS', webUrl: 'https://tc/1' }),
    ),
    jobsStatus: vi.fn(() => okAsync([])),
    jobRefs: vi.fn(() => okAsync([{ name: 'next', kind: 'branch' }])),
    exactJobRef: vi.fn(() => okAsync({ name: 'next', kind: 'branch' })),
    jobParameters: vi.fn(() => okAsync({ parameters: [], schemaRevision: 'sha' })),
    triggerJob: vi.fn(() =>
      okAsync({
        provider: 'github-actions',
        runId: '1',
        webUrl: 'https://github.com/run/1',
        ref: { name: 'next', kind: 'branch' },
      }),
    ),
    runActivity: vi.fn(() => okAsync({ running: [], queued: [], recent: [] })),
    runById: vi.fn(() =>
      okAsync({
        provider: 'github-actions',
        runId: '1',
        jobId: '.github/workflows/release.yml',
        jobLabel: 'Release',
        state: 'queued',
        conclusion: 'unknown',
        webUrl: '',
      }),
    ),
    githubSetup: vi.fn(() =>
      okAsync({
        repository: 'itsoltech/canopy-desktop',
        defaultBranch: 'next',
        workflows: [],
      }),
    ),
    testGitHubConnection: vi.fn(() => okAsync(undefined)),
    saveGitHubCredential: vi.fn(() => okAsync(undefined)),
    prepareTeamCityConfigApproval: vi.fn(() =>
      ok({ ...currentCredential, approvalRequired: teamCityConfigApprovalRequired }),
    ),
    prepareTeamCityDiscoveryApproval: vi.fn(() =>
      ok({ ...currentCredential, approvalRequired: teamCityDiscoveryApprovalRequired }),
    ),
    approveTeamCityConfig: vi.fn((_repoRoot, ci, expected) =>
      expected.credentialId === currentCredential.credentialId &&
      expected.revision === currentCredential.revision
        ? ok(undefined)
        : err({ _tag: 'CiCredentialApprovalRequired' as const, baseUrl: ci.baseUrl }),
    ),
    approveTeamCityDiscovery: vi.fn((_repoRoot, baseUrl, expected) =>
      expected.credentialId === currentCredential.credentialId &&
      expected.revision === currentCredential.revision
        ? ok(undefined)
        : err({ _tag: 'CiCredentialApprovalRequired' as const, baseUrl }),
    ),
  } as unknown as CiManager
  // The workspace gate: only paths under /ws are inside the sender's workspaces,
  // and authorization RESOLVES the path (realpath) — downstream must use that form.
  const validatePathAccess = vi.fn(async (_wcId: number, target: string) => {
    if (!target.startsWith('/ws/')) throw new Error('Access denied: path outside workspace')
    return `/resolved${target}`
  })
  const confirmGitHubDispatch = vi.fn(async () => true)
  const confirmTeamCityAccess = vi.fn(async () => {
    if (rotateTeamCityCredentialDuringConfirmation) {
      currentCredential = { credentialId: 'cred-1', revision: 'revision-2' }
    }
    return true
  })
  const teamCityOriginTrust = {
    requiresApproval: vi.fn(async () => privateOriginApprovalRequired),
    approve: vi.fn(),
    ensureAllowed: vi.fn(() => okAsync({ allowPrivate: false })),
  }
  registerCiHandlers({
    ipcMain,
    ciManager,
    validatePathAccess,
    ...(nativeConfirmation ? { confirmGitHubDispatch } : {}),
    ...(teamCityConfirmation ? { confirmTeamCityAccess } : {}),
    teamCityOriginTrust,
  })
  return {
    invoke: (channel, payload) => {
      const listener = handlers.get(channel)
      if (!listener) throw new Error(`no handler for ${channel}`)
      return Promise.resolve(listener({ sender: { id: 7 } }, payload))
    },
    ciManager,
    validatePathAccess,
    confirmGitHubDispatch,
    confirmTeamCityAccess,
    teamCityOriginTrust,
  }
}

// Every repo-scoped channel with a minimal otherwise-valid payload, and the
// CiManager method it lands on (the resolved-path passthrough loops over this).
const REPO_SCOPED: Array<{
  channel: string
  method: keyof CiManager
  payload: (repoRoot: string) => unknown
}> = [
  { channel: 'ci:config', method: 'loadConfig', payload: (repoRoot) => ({ repoRoot }) },
  {
    channel: 'ci:status',
    method: 'loadConfig',
    payload: (repoRoot) => ({ repoRoot, branch: 'next' }),
  },
  {
    channel: 'ci:trigger',
    method: 'trigger',
    payload: (repoRoot) => ({ repoRoot, buildTypeId: 'Gakko_Build', branch: 'next' }),
  },
  { channel: 'ci:activity', method: 'activity', payload: (repoRoot) => ({ repoRoot }) },
  {
    channel: 'ci:branches',
    method: 'branches',
    payload: (repoRoot) => ({ repoRoot, buildTypeId: 'Gakko_Build' }),
  },
  {
    channel: 'ci:buildParameters',
    method: 'promptParameters',
    payload: (repoRoot) => ({ repoRoot, buildTypeId: 'Gakko_Build' }),
  },
  {
    channel: 'ci:saveConfig',
    method: 'saveConfig',
    payload: (repoRoot) => ({
      repoRoot,
      ci: { baseUrl: 'https://tc', buildTypes: [{ id: 'Gakko_Build', label: 'B' }] },
    }),
  },
  {
    channel: 'ci:build',
    method: 'build',
    payload: (repoRoot) => ({
      repoRoot,
      expectedBaseUrl: 'https://tc.example.com',
      buildId: 1,
    }),
  },
  {
    channel: 'ci:listBuildTypes',
    method: 'listBuildTypes',
    payload: (repoRoot) => ({ repoRoot, baseUrl: 'https://tc.example.com' }),
  },
  {
    channel: 'ci:jobsStatus',
    method: 'jobsStatus',
    payload: (repoRoot) => ({ repoRoot, ref: { name: 'next', kind: 'branch' } }),
  },
  {
    channel: 'ci:jobRefs',
    method: 'jobRefs',
    payload: (repoRoot) => ({ repoRoot, jobId: '.github/workflows/release.yml' }),
  },
  {
    channel: 'ci:exactJobRef',
    method: 'exactJobRef',
    payload: (repoRoot) => ({
      repoRoot,
      jobId: '.github/workflows/release.yml',
      name: 'next',
    }),
  },
  {
    channel: 'ci:jobParameters',
    method: 'jobParameters',
    payload: (repoRoot) => ({
      repoRoot,
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' },
    }),
  },
  {
    channel: 'ci:triggerJob',
    method: 'triggerJob',
    payload: (repoRoot) => ({
      repoRoot,
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' },
      schemaRevision: 'sha',
      inputs: { dry_run: true },
    }),
  },
  { channel: 'ci:runActivity', method: 'runActivity', payload: (repoRoot) => ({ repoRoot }) },
  {
    channel: 'ci:run',
    method: 'runById',
    payload: (repoRoot) => ({ repoRoot, runId: '123' }),
  },
  { channel: 'ci:githubSetup', method: 'githubSetup', payload: (repoRoot) => ({ repoRoot }) },
  {
    channel: 'ci:testGitHubConnection',
    method: 'testGitHubConnection',
    payload: (repoRoot) => ({ repoRoot, token: 'token' }),
  },
  {
    channel: 'ci:setGitHubCredential',
    method: 'saveGitHubCredential',
    payload: (repoRoot) => ({ repoRoot, token: 'token' }),
  },
]

describe('CI IPC authorization', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  for (const { channel, payload } of REPO_SCOPED) {
    it(`${channel} rejects a repoRoot outside the workspace before touching CiManager`, async () => {
      const { invoke, ciManager } = harness()
      await expect(invoke(channel, payload('/outside/evil'))).rejects.toThrow(
        'Access denied: path outside workspace',
      )
      for (const fn of Object.values(ciManager)) {
        expect(fn).not.toHaveBeenCalled()
      }
    })
  }

  for (const { channel, method, payload } of REPO_SCOPED) {
    it(`${channel} passes the RESOLVED path downstream, never the renderer string`, async () => {
      const { invoke, ciManager, validatePathAccess } = harness()
      await invoke(channel, payload('/ws/repo'))
      expect(validatePathAccess).toHaveBeenCalledWith(7, '/ws/repo')
      // The gate's return value is the only form allowed past it: passing the raw
      // string back would re-open the TOCTOU the realpath resolution closes.
      expect(vi.mocked(ciManager[method]).mock.calls[0][0]).toBe('/resolved/ws/repo')
    })
  }

  it('candidate-token connection tests stay path-free', async () => {
    const { invoke, ciManager, validatePathAccess, confirmTeamCityAccess } = harness()
    await invoke('ci:testNewConnection', { baseUrl: 'https://tc.example.com', token: 'token' })
    expect(ciManager.listBuildTypes).not.toHaveBeenCalled()
    expect(validatePathAccess).not.toHaveBeenCalled()
    expect(confirmTeamCityAccess).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        action: 'test-connection',
        baseUrl: 'https://tc.example.com',
        usesStoredCredential: false,
        privateOrigin: false,
      }),
    )
  })

  it('requires trusted repository approval before a stored token can discover jobs', async () => {
    const { invoke, ciManager, confirmTeamCityAccess } = harness({
      teamCityDiscoveryApprovalRequired: true,
    })

    await invoke('ci:listBuildTypes', {
      repoRoot: '/ws/repo',
      baseUrl: 'https://tc.example.com',
    })

    expect(confirmTeamCityAccess).toHaveBeenCalledWith(
      expect.objectContaining({ sender: { id: 7 } }),
      expect.objectContaining({
        action: 'discover-build-types',
        repoRoot: '/resolved/ws/repo',
        baseUrl: 'https://tc.example.com',
        usesStoredCredential: true,
      }),
    )
    expect(ciManager.approveTeamCityDiscovery).toHaveBeenCalledWith(
      '/resolved/ws/repo',
      'https://tc.example.com',
      expect.objectContaining({ credentialId: 'cred-1', revision: 'revision-1' }),
    )
    expect(ciManager.listBuildTypes).toHaveBeenCalledWith(
      '/resolved/ws/repo',
      'https://tc.example.com',
    )
  })

  it('does not discover jobs when repository credential approval is declined', async () => {
    const { invoke, ciManager, confirmTeamCityAccess } = harness({
      teamCityDiscoveryApprovalRequired: true,
    })
    confirmTeamCityAccess.mockResolvedValue(false)

    await expect(
      invoke('ci:listBuildTypes', {
        repoRoot: '/ws/repo',
        baseUrl: 'https://tc.example.com',
      }),
    ).rejects.toThrow('not approved')

    expect(ciManager.approveTeamCityDiscovery).not.toHaveBeenCalled()
    expect(ciManager.listBuildTypes).not.toHaveBeenCalled()
  })

  it('does not bind or discover when the credential changes during confirmation', async () => {
    const { invoke, ciManager } = harness({
      teamCityDiscoveryApprovalRequired: true,
      rotateTeamCityCredentialDuringConfirmation: true,
    })

    await expect(
      invoke('ci:listBuildTypes', {
        repoRoot: '/ws/repo',
        baseUrl: 'https://tc.example.com',
      }),
    ).rejects.toThrow(/Approve/i)

    expect(ciManager.approveTeamCityDiscovery).toHaveBeenCalledWith(
      '/resolved/ws/repo',
      'https://tc.example.com',
      expect.objectContaining({ credentialId: 'cred-1', revision: 'revision-1' }),
    )
    expect(ciManager.listBuildTypes).not.toHaveBeenCalled()
  })

  it('combines exact config credential consent with private-origin consent', async () => {
    const { invoke, ciManager, confirmTeamCityAccess, teamCityOriginTrust } = harness({
      teamCityConfigApprovalRequired: true,
      privateOriginApprovalRequired: true,
    })

    await invoke('ci:saveConfig', {
      repoRoot: '/ws/repo',
      ci: {
        baseUrl: 'https://teamcity.internal',
        buildTypes: [{ id: 'Deploy', label: 'Deploy' }],
      },
    })

    expect(confirmTeamCityAccess).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        action: 'save-config',
        repoRoot: '/resolved/ws/repo',
        baseUrl: 'https://teamcity.internal',
        buildTypes: [{ id: 'Deploy', label: 'Deploy' }],
        usesStoredCredential: true,
        privateOrigin: true,
      }),
    )
    expect(teamCityOriginTrust.approve).toHaveBeenCalledWith('https://teamcity.internal')
    expect(ciManager.approveTeamCityConfig).toHaveBeenCalledWith(
      '/resolved/ws/repo',
      expect.objectContaining({ buildTypes: [{ id: 'Deploy', label: 'Deploy' }] }),
      expect.objectContaining({ credentialId: 'cred-1', revision: 'revision-1' }),
    )
    expect(ciManager.saveConfig).toHaveBeenCalled()
  })

  it('requires private-origin consent before testing a candidate token', async () => {
    const { invoke, confirmTeamCityAccess, teamCityOriginTrust } = harness({
      privateOriginApprovalRequired: true,
    })

    await invoke('ci:testNewConnection', {
      baseUrl: 'https://teamcity.internal',
      token: 'token',
    })

    expect(confirmTeamCityAccess).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        action: 'test-connection',
        baseUrl: 'https://teamcity.internal',
        usesStoredCredential: false,
        privateOrigin: true,
      }),
    )
    expect(teamCityOriginTrust.approve).toHaveBeenCalledWith('https://teamcity.internal')
    expect(ciTestConnection).toHaveBeenCalledOnce()
  })

  it('trims candidate TeamCity tokens before testing the connection', async () => {
    const { invoke } = harness()

    await invoke('ci:testNewConnection', {
      baseUrl: 'https://tc.example.com/',
      token: '  token  ',
    })

    expect(ciTestConnection).toHaveBeenCalledWith('https://tc.example.com', 'token', {
      allowPrivate: false,
    })
  })

  it('does not send a candidate token to a public origin when native consent is declined', async () => {
    const { invoke, confirmTeamCityAccess } = harness()
    confirmTeamCityAccess.mockResolvedValue(false)

    await expect(
      invoke('ci:testNewConnection', {
        baseUrl: 'https://tc.example.com',
        token: 'token',
      }),
    ).rejects.toThrow('not approved')

    expect(ciTestConnection).not.toHaveBeenCalled()
  })

  it('rejects duplicate TeamCity build ids before approval or config writes', async () => {
    const { invoke, ciManager, confirmTeamCityAccess } = harness({
      teamCityConfigApprovalRequired: true,
    })

    await expect(
      invoke('ci:saveConfig', {
        repoRoot: '/ws/repo',
        ci: {
          baseUrl: 'https://tc.example.com',
          buildTypes: [
            { id: 'Deploy', label: 'Deploy one' },
            { id: 'Deploy', label: 'Deploy two' },
          ],
        },
      }),
    ).rejects.toThrow('Duplicate TeamCity build configuration')

    expect(ciManager.prepareTeamCityConfigApproval).not.toHaveBeenCalled()
    expect(confirmTeamCityAccess).not.toHaveBeenCalled()
    expect(ciManager.saveConfig).not.toHaveBeenCalled()
  })

  it('rejects oversized raw TeamCity tokens before trimming or making a request', async () => {
    const { invoke } = harness()

    await expect(
      invoke('ci:testNewConnection', {
        baseUrl: 'https://tc.example.com',
        token: ' '.repeat(10_001),
      }),
    ).rejects.toThrow('Invalid TeamCity token')

    expect(ciTestConnection).not.toHaveBeenCalled()
  })

  it('rejects whitespace-only TeamCity tokens before making a request', async () => {
    const { invoke } = harness()

    await expect(
      invoke('ci:testNewConnection', {
        baseUrl: 'https://tc.example.com',
        token: '  \r\n  ',
      }),
    ).rejects.toThrow('TeamCity token is required')

    expect(ciTestConnection).not.toHaveBeenCalled()
  })

  it('passes a trusted confirmation callback to direct trigger IPC calls', async () => {
    const { invoke, ciManager, confirmGitHubDispatch } = harness()
    await invoke('ci:triggerJob', {
      repoRoot: '/ws/repo',
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' },
      schemaRevision: 'sha',
      inputs: { dry_run: true },
    })

    expect(ciManager.triggerJob).toHaveBeenCalledOnce()
    const confirm = vi.mocked(ciManager.triggerJob).mock.calls[0]?.[2]
    expect(confirm).toBeTypeOf('function')
    const details = {
      repository: 'itsoltech/canopy-desktop',
      workflowPath: '.github/workflows/release.yml',
      workflowLabel: 'Release',
      ref: { name: 'next', kind: 'branch' as const },
      inputs: { dry_run: true },
    }
    expect(await confirm?.(details)).toBe(true)
    expect(confirmGitHubDispatch).toHaveBeenCalledWith(
      expect.objectContaining({ sender: { id: 7 } }),
      details,
    )
  })

  it('pins the shipped path without an optional native confirmation callback', async () => {
    const { invoke, ciManager, confirmGitHubDispatch } = harness({ nativeConfirmation: false })
    await invoke('ci:triggerJob', {
      repoRoot: '/ws/repo',
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' },
      schemaRevision: 'sha',
      inputs: { dry_run: true },
    })

    expect(ciManager.triggerJob).toHaveBeenCalledOnce()
    const confirm = vi.mocked(ciManager.triggerJob).mock.calls[0]?.[2]
    expect(confirm).toBeUndefined()
    expect(confirmGitHubDispatch).not.toHaveBeenCalled()
  })

  it('returns a structured success from the trigger channel', async () => {
    const { invoke } = harness()

    await expect(
      invoke('ci:triggerJob', {
        repoRoot: '/ws/repo',
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next', kind: 'branch' },
        schemaRevision: 'sha',
        inputs: {},
      }),
    ).resolves.toEqual({
      ok: true,
      value: expect.objectContaining({ runId: '1' }),
    })
  })

  it('returns a stable error code instead of forcing the renderer to parse messages', async () => {
    const { invoke, ciManager } = harness()
    vi.mocked(ciManager.triggerJob).mockReturnValue(errAsync({ _tag: 'CiWorkflowSchemaChanged' }))

    await expect(
      invoke('ci:triggerJob', {
        repoRoot: '/ws/repo',
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next', kind: 'branch' },
        schemaRevision: 'stale',
        inputs: {},
      }),
    ).resolves.toEqual({
      ok: false,
      error: expect.objectContaining({ code: 'CiWorkflowSchemaChanged' }),
    })
  })

  it('preserves an API status separately from provider-controlled error text', async () => {
    const { invoke, ciManager } = harness()
    vi.mocked(ciManager.triggerJob).mockReturnValue(
      errAsync({
        _tag: 'CiApiError',
        status: 502,
        message: 'Forbidden appeared in an upstream proxy response',
      }),
    )

    await expect(
      invoke('ci:triggerJob', {
        repoRoot: '/ws/repo',
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next', kind: 'branch' },
        schemaRevision: 'sha',
        inputs: {},
      }),
    ).resolves.toEqual({
      ok: false,
      error: expect.objectContaining({
        code: 'CiApiError',
        status: 502,
        message: expect.stringContaining('Forbidden'),
      }),
    })
  })

  it('trims GitHub tokens before testing or storing them', async () => {
    const { invoke, ciManager } = harness()

    await invoke('ci:testGitHubConnection', { repoRoot: '/ws/repo', token: '  token  ' })
    await invoke('ci:setGitHubCredential', { repoRoot: '/ws/repo', token: '  token  ' })

    expect(ciManager.testGitHubConnection).toHaveBeenCalledWith('/resolved/ws/repo', 'token')
    expect(ciManager.saveGitHubCredential).toHaveBeenCalledWith('/resolved/ws/repo', 'token')
  })

  it('rejects oversized raw GitHub tokens before trimming or calling CiManager', async () => {
    const { invoke, ciManager } = harness()

    await expect(
      invoke('ci:testGitHubConnection', { repoRoot: '/ws/repo', token: ' '.repeat(10_001) }),
    ).rejects.toThrow('Invalid GitHub token')

    expect(ciManager.testGitHubConnection).not.toHaveBeenCalled()
  })

  it('rejects kind-less refs and non-primitive inputs before CiManager', async () => {
    const { invoke, ciManager } = harness()
    await expect(
      invoke('ci:triggerJob', {
        repoRoot: '/ws/repo',
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next' },
        schemaRevision: 'sha',
        inputs: { dry_run: { nested: true } },
      }),
    ).rejects.toThrow('Invalid CI ref kind')
    expect(ciManager.triggerJob).not.toHaveBeenCalled()
  })

  it.each(['release+qa', 'hello-$USER', 'release#1', 'percent%done'])(
    'accepts a GitHub picker ref containing valid Git characters: %s',
    async (refName) => {
      const { invoke, ciManager } = harness()

      await invoke('ci:triggerJob', {
        repoRoot: '/ws/repo',
        jobId: '.github/workflows/release.yml',
        ref: { name: refName, kind: 'branch' },
        schemaRevision: 'sha',
        inputs: {},
      })

      expect(ciManager.triggerJob).toHaveBeenCalledWith(
        '/resolved/ws/repo',
        expect.objectContaining({ ref: { name: refName, kind: 'branch' } }),
        expect.any(Function),
      )
    },
  )

  it.each(['release+qa', 'hello-$USER'])(
    'accepts a valid Git ref in legacy TeamCity channels: %s',
    async (branch) => {
      const { invoke, ciManager } = harness()

      const status = await invoke('ci:status', { repoRoot: '/ws/repo', branch })
      await invoke('ci:trigger', {
        repoRoot: '/ws/repo',
        buildTypeId: 'Gakko_Build',
        branch,
      })

      expect(status).toMatchObject({ configured: true, rows: [] })
      expect(ciManager.statusFor).toHaveBeenCalledWith(
        '/resolved/ws/repo',
        expect.any(Object),
        branch,
      )
      expect(ciManager.trigger).toHaveBeenCalledWith(
        '/resolved/ws/repo',
        'Gakko_Build',
        branch,
        undefined,
      )
    },
  )

  it('reports an unusable TeamCity ref without pretending the repository is unconfigured', async () => {
    const { invoke, ciManager } = harness()

    await expect(
      invoke('ci:status', { repoRoot: '/ws/repo', branch: 'invalid:branch' }),
    ).resolves.toMatchObject({
      configured: true,
      rows: [],
      error: 'Invalid branch name',
    })
    expect(ciManager.statusFor).not.toHaveBeenCalled()
  })

  it('returns a stable ambiguous code for a TeamCity trigger that may have been accepted', async () => {
    const { invoke, ciManager } = harness()
    vi.mocked(ciManager.trigger).mockReturnValue(
      errAsync({
        _tag: 'CiDispatchAmbiguous',
        provider: 'teamcity',
        detailsUrl: 'https://tc.example.com/viewType.html?buildTypeId=Deploy',
      }),
    )

    await expect(
      invoke('ci:trigger', {
        repoRoot: '/ws/repo',
        buildTypeId: 'Deploy',
        branch: 'next',
      }),
    ).resolves.toEqual({
      ok: false,
      error: expect.objectContaining({
        code: 'CiDispatchAmbiguous',
        message: expect.stringContaining('Check TeamCity'),
      }),
    })
  })

  it('rejects locator-unsafe refs on the shipped legacy TeamCity status channel', async () => {
    const { invoke, ciManager } = harness()

    await expect(
      invoke('ci:status', { repoRoot: '/ws/repo', branch: 'feat(ci),v2' }),
    ).resolves.toMatchObject({
      configured: true,
      rows: [],
      error: 'TeamCity branch contains locator-unsafe characters',
    })
    expect(ciManager.statusFor).not.toHaveBeenCalled()
  })
})
