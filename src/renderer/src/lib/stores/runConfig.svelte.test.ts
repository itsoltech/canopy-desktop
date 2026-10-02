import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const workspace = vi.hoisted(() => ({
  workspaceState: {
    repoRoot: '/repo' as string | null,
    selectedWorktreePath: '/repo' as string | null,
  },
}))
vi.mock('./workspace.svelte', () => workspace)
vi.mock('./toast.svelte', () => ({ addToast: vi.fn() }))

import {
  discoverConfigs,
  executeRunConfig,
  getSelectedConfig,
  getSources,
  selectRunConfig,
} from './runConfig.svelte'

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => (resolve = r))
  return { promise, resolve }
}

interface DiscoveredSource {
  configDir: string
  relativePath: string
  file: { configurations: Array<{ name: string; command: string }> }
}

const source = (configDir: string, names: string[]): DiscoveredSource => ({
  configDir,
  relativePath: '.canopy/run.toml',
  file: { configurations: names.map((name) => ({ name, command: 'npm run dev' })) },
})

describe('run configuration store', () => {
  let api: Record<string, ReturnType<typeof vi.fn>>

  beforeEach(() => {
    workspace.workspaceState.repoRoot = '/repo'
    workspace.workspaceState.selectedWorktreePath = '/repo'
    api = {
      runConfigDiscover: vi.fn(),
      runConfigExecuteCommand: vi.fn(),
      runConfigListRunning: vi.fn(async () => []),
    }
    vi.stubGlobal('window', { api })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('reports the worktree the run started in, even if the selection changed meanwhile', async () => {
    const started = deferred<{ sessionId: string }>()
    api.runConfigExecuteCommand.mockReturnValueOnce(started.promise)

    const pending = executeRunConfig('/repo/.canopy', 'dev')
    workspace.workspaceState.selectedWorktreePath = '/repo-feature'
    started.resolve({ sessionId: 'pty-1' })

    expect(await pending).toEqual({ sessionId: 'pty-1', worktreePath: '/repo' })
  })

  it('ignores a second start of the same configuration while the first is starting', async () => {
    const started = deferred<{ sessionId: string }>()
    api.runConfigExecuteCommand.mockReturnValueOnce(started.promise)

    const first = executeRunConfig('/repo/.canopy', 'dev')
    const second = await executeRunConfig('/repo/.canopy', 'dev')
    started.resolve({ sessionId: 'pty-1' })
    await first

    expect(second).toBeNull()
    expect(api.runConfigExecuteCommand).toHaveBeenCalledTimes(1)
  })

  it('keeps the newest discovery when an older one resolves last', async () => {
    const older = deferred<DiscoveredSource[]>()
    api.runConfigDiscover
      .mockReturnValueOnce(older.promise)
      .mockResolvedValueOnce([source('/other/.canopy', ['serve'])])

    const first = discoverConfigs()
    workspace.workspaceState.repoRoot = '/other'
    await discoverConfigs()
    older.resolve([source('/repo/.canopy', ['dev'])])
    await first

    expect(getSources().map((s) => s.configDir)).toEqual(['/other/.canopy'])
  })

  it('drops a selection whose configuration no longer exists', async () => {
    selectRunConfig('/repo/.canopy', 'dev')
    api.runConfigDiscover.mockResolvedValueOnce([source('/repo/.canopy', ['test'])])

    await discoverConfigs()

    expect(getSelectedConfig()).toBeNull()
  })
})
