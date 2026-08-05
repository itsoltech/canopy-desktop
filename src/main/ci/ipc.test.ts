import { describe, expect, it, vi, beforeEach } from 'vitest'
import { errAsync, okAsync } from 'neverthrow'
import { registerCiHandlers } from './ipc'
import type { CiManager } from './CiManager'
import { testConnection as ciTestConnection } from './teamcity'

// Pins the AUTHORIZATION contract of the repo-scoped CI channels: a renderer-
// supplied repoRoot outside the sender's workspaces must be rejected BEFORE any
// CiManager call (no config read, no token use, no write), and on success only
// the RESOLVED path may flow downstream.

vi.mock('./teamcity', () => ({
  testConnection: vi.fn(() => okAsync(undefined)),
}))

function harness(): {
  invoke: (channel: string, payload: unknown) => Promise<unknown>
  ciManager: CiManager
  validatePathAccess: ReturnType<typeof vi.fn>
  confirmGitHubDispatch: ReturnType<typeof vi.fn>
} {
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
  } as unknown as CiManager
  // The workspace gate: only paths under /ws are inside the sender's workspaces,
  // and authorization RESOLVES the path (realpath) — downstream must use that form.
  const validatePathAccess = vi.fn(async (_wcId: number, target: string) => {
    if (!target.startsWith('/ws/')) throw new Error('Access denied: path outside workspace')
    return `/resolved${target}`
  })
  const confirmGitHubDispatch = vi.fn(async () => true)
  registerCiHandlers({ ipcMain, ciManager, validatePathAccess, confirmGitHubDispatch })
  return {
    invoke: (channel, payload) => {
      const listener = handlers.get(channel)
      if (!listener) throw new Error(`no handler for ${channel}`)
      return Promise.resolve(listener({ sender: { id: 7 } }, payload))
    },
    ciManager,
    validatePathAccess,
    confirmGitHubDispatch,
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
  { channel: 'ci:build', method: 'build', payload: (repoRoot) => ({ repoRoot, buildId: 1 }) },
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

  it('URL-scoped channels stay path-free (no workspace gate involved)', async () => {
    const { invoke, ciManager, validatePathAccess } = harness()
    await invoke('ci:listBuildTypes', { baseUrl: 'https://tc.example.com' })
    expect(ciManager.listBuildTypes).toHaveBeenCalledWith('https://tc.example.com')
    expect(validatePathAccess).not.toHaveBeenCalled()
  })

  it('trims candidate TeamCity tokens before testing the connection', async () => {
    const { invoke } = harness()

    await invoke('ci:testNewConnection', {
      baseUrl: 'https://tc.example.com/',
      token: '  token  ',
    })

    expect(ciTestConnection).toHaveBeenCalledWith('https://tc.example.com', 'token')
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

  it.each(['release+qa', 'hello-$USER'])(
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
})
