import { describe, expect, it, vi, beforeEach } from 'vitest'
import { okAsync } from 'neverthrow'
import { registerCiHandlers } from './ipc'
import type { CiManager } from './CiManager'

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
  } as unknown as CiManager
  // The workspace gate: only paths under /ws are inside the sender's workspaces,
  // and authorization RESOLVES the path (realpath) — downstream must use that form.
  const validatePathAccess = vi.fn(async (_wcId: number, target: string) => {
    if (!target.startsWith('/ws/')) throw new Error('Access denied: path outside workspace')
    return `/resolved${target}`
  })
  registerCiHandlers({ ipcMain, ciManager, validatePathAccess })
  return {
    invoke: (channel, payload) => {
      const listener = handlers.get(channel)
      if (!listener) throw new Error(`no handler for ${channel}`)
      return Promise.resolve(listener({ sender: { id: 7 } }, payload))
    },
    ciManager,
    validatePathAccess,
  }
}

// Every repo-scoped channel with a minimal otherwise-valid payload.
const REPO_SCOPED: Array<{ channel: string; payload: (repoRoot: string) => unknown }> = [
  { channel: 'ci:config', payload: (repoRoot) => ({ repoRoot }) },
  { channel: 'ci:status', payload: (repoRoot) => ({ repoRoot, branch: 'next' }) },
  {
    channel: 'ci:trigger',
    payload: (repoRoot) => ({ repoRoot, buildTypeId: 'Gakko_Build', branch: 'next' }),
  },
  { channel: 'ci:activity', payload: (repoRoot) => ({ repoRoot }) },
  { channel: 'ci:branches', payload: (repoRoot) => ({ repoRoot, buildTypeId: 'Gakko_Build' }) },
  {
    channel: 'ci:buildParameters',
    payload: (repoRoot) => ({ repoRoot, buildTypeId: 'Gakko_Build' }),
  },
  {
    channel: 'ci:saveConfig',
    payload: (repoRoot) => ({
      repoRoot,
      ci: { baseUrl: 'https://tc', buildTypes: [{ id: 'Gakko_Build', label: 'B' }] },
    }),
  },
  { channel: 'ci:build', payload: (repoRoot) => ({ repoRoot, buildId: 1 }) },
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

  it('passes only the RESOLVED path downstream, never the renderer-supplied string', async () => {
    const { invoke, ciManager, validatePathAccess } = harness()
    await invoke('ci:config', { repoRoot: '/ws/repo' })
    expect(validatePathAccess).toHaveBeenCalledWith(7, '/ws/repo')
    expect(ciManager.loadConfig).toHaveBeenCalledWith('/resolved/ws/repo')
    await invoke('ci:saveConfig', { repoRoot: '/ws/repo', ci: null })
    expect(ciManager.saveConfig).toHaveBeenCalledWith('/resolved/ws/repo', null)
  })

  it('URL-scoped channels stay path-free (no workspace gate involved)', async () => {
    const { invoke, ciManager, validatePathAccess } = harness()
    await invoke('ci:listBuildTypes', { baseUrl: 'https://tc.example.com' })
    expect(ciManager.listBuildTypes).toHaveBeenCalledWith('https://tc.example.com')
    expect(validatePathAccess).not.toHaveBeenCalled()
  })
})
