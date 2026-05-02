import assert from 'node:assert/strict'
import { test } from 'vitest'
import type { IpcMainInvokeEvent } from 'electron'
import { Effect, Exit } from 'effect'
import {
  createToolSpawnEffect,
  type ToolSpawnEffectDeps,
  type ToolSpawnError,
} from './toolSpawnEffect'

function createShellSpawnDeps(overrides: Partial<ToolSpawnEffectDeps> = {}): ToolSpawnEffectDeps {
  const killed: string[] = []
  const tmuxKilled: string[] = []
  const tmuxCreated: string[] = []
  const untracked: string[] = []

  const deps = {
    ptyManager: {
      spawn: () => ({
        id: 'pty-1',
        pty: {
          onExit: () => undefined,
        },
      }),
      kill: (sessionId: string) => {
        killed.push(sessionId)
      },
      __killed: killed,
    },
    wsBridge: {
      create: async () => {
        throw new Error('ws failed')
      },
      destroy: () => undefined,
    },
    workspaceStore: {
      getByPath: () => undefined,
    },
    preferencesStore: {
      get: () => undefined,
    },
    toolRegistry: {
      get: () => ({ id: 'shell', name: 'Shell', command: 'shell', args: [] }),
      resolveCommand: () => '/bin/sh',
    },
    agentSessionManager: {
      isAgentTool: () => false,
    },
    windowManager: {
      trackPtySession: () => undefined,
      untrackPtySession: (_windowId: number, sessionId: string) => {
        untracked.push(sessionId)
      },
      __untracked: untracked,
    },
    tmuxManager: {
      isAvailable: async () => false,
      attachArgs: () => ({ command: 'tmux', args: [] }),
      newSession: async ({ name }: { name: string }) => {
        tmuxCreated.push(name)
      },
      killSession: async (name: string) => {
        tmuxKilled.push(name)
      },
      __created: tmuxCreated,
      __killed: tmuxKilled,
    },
    profileStore: {
      getInternal: async () => undefined,
    },
    ...overrides,
  }

  return deps as unknown as ToolSpawnEffectDeps
}

function createIpcEvent(): IpcMainInvokeEvent {
  return {
    sender: {
      id: 7,
      isDestroyed: () => false,
      send: () => undefined,
    },
  } as unknown as IpcMainInvokeEvent
}

test('tool:spawn cleans up acquired pty when websocket bridge acquisition fails', async () => {
  const deps = createShellSpawnDeps()
  const program = createToolSpawnEffect(deps, createIpcEvent(), {
    toolId: 'shell',
    worktreePath: '/tmp/canopy-worktree',
  })
  const failure = await Effect.runPromise(Effect.flip(program))

  assert.equal((failure as ToolSpawnError)._tag, 'ToolSpawnError')
  assert.equal((failure as ToolSpawnError).stage, 'websocket-bridge')
  assert.deepEqual((deps.ptyManager as unknown as { __killed: string[] }).__killed, ['pty-1'])
  assert.deepEqual((deps.windowManager as unknown as { __untracked: string[] }).__untracked, [])
})

test('tool:spawn cleans up acquired tmux session when websocket bridge acquisition fails', async () => {
  const deps = createShellSpawnDeps({
    preferencesStore: {
      get: (key: string) => (key === 'tmux.enabled' ? 'true' : null),
    } as unknown as ToolSpawnEffectDeps['preferencesStore'],
    workspaceStore: {
      getByPath: () => ({ id: 'workspace-1' }),
    } as unknown as ToolSpawnEffectDeps['workspaceStore'],
    tmuxManager: {
      isAvailable: async () => true,
      attachArgs: (name: string) => ({ command: 'tmux', args: ['attach', '-t', name] }),
      newSession: async ({ name }: { name: string }) => {
        ;(deps.tmuxManager as unknown as { __created: string[] }).__created.push(name)
      },
      killSession: async (name: string) => {
        ;(deps.tmuxManager as unknown as { __killed: string[] }).__killed.push(name)
      },
      __created: [],
      __killed: [],
    } as unknown as ToolSpawnEffectDeps['tmuxManager'],
  })

  const exit = await Effect.runPromiseExit(
    createToolSpawnEffect(deps, createIpcEvent(), {
      toolId: 'shell',
      worktreePath: '/tmp/canopy-worktree',
    }),
  )

  assert.equal(Exit.isFailure(exit), true)
  const tmuxCreated = (deps.tmuxManager as unknown as { __created: string[] }).__created
  const tmuxKilled = (deps.tmuxManager as unknown as { __killed: string[] }).__killed
  assert.equal(tmuxCreated.length, 1, 'expected exactly one tmux session created')
  assert.match(tmuxCreated[0], /^canopy-workspace-/)
  assert.deepEqual(tmuxKilled, tmuxCreated, 'killed sessions must match created sessions')
  assert.deepEqual((deps.ptyManager as unknown as { __killed: string[] }).__killed, ['pty-1'])
})

test('tool:spawn tmux finalizer swallows killSession rejection and preserves primary error', async () => {
  // The tmux release finalizer must be infallible. If killSession rejects
  // (e.g. tmux server already gone), Effect must still surface the primary
  // websocket-bridge error, not a defect from the finalizer.
  const deps = createShellSpawnDeps({
    preferencesStore: {
      get: (key: string) => (key === 'tmux.enabled' ? 'true' : null),
    } as unknown as ToolSpawnEffectDeps['preferencesStore'],
    workspaceStore: {
      getByPath: () => ({ id: 'workspace-finalizer' }),
    } as unknown as ToolSpawnEffectDeps['workspaceStore'],
    tmuxManager: {
      isAvailable: async () => true,
      attachArgs: (name: string) => ({ command: 'tmux', args: ['attach', '-t', name] }),
      newSession: async () => undefined,
      killSession: async () => {
        throw new Error('tmux server gone')
      },
    } as unknown as ToolSpawnEffectDeps['tmuxManager'],
  })

  const exit = await Effect.runPromiseExit(
    createToolSpawnEffect(deps, createIpcEvent(), {
      toolId: 'shell',
      worktreePath: '/tmp/canopy-worktree',
    }),
  )

  // Primary failure must be the typed ToolSpawnError from the websocket
  // bridge stage — NOT a defect from the rejected killSession. If the
  // finalizer were not infallible, Exit would carry a Die node from the
  // rejected promise instead of the original Fail.
  assert.equal(Exit.isFailure(exit), true)
  if (Exit.isFailure(exit)) {
    // Effect's Cause should report a typed failure, not a defect.
    const failures: ToolSpawnError[] = []
    const walk = (cause: unknown): void => {
      if (!cause || typeof cause !== 'object') return
      const c = cause as { _tag?: string; error?: unknown; left?: unknown; right?: unknown }
      if (c._tag === 'Fail' && c.error) failures.push(c.error as ToolSpawnError)
      if ('left' in c) walk(c.left)
      if ('right' in c) walk(c.right)
    }
    walk(exit.cause)
    assert.equal(failures.length >= 1, true, 'expected a typed failure in cause')
    assert.equal(failures[0]._tag, 'ToolSpawnError')
    assert.equal(failures[0].stage, 'websocket-bridge')
  }
})

test('tool:spawn destroys acquired agent session when later setup fails', async () => {
  const destroyed: string[] = []
  const ptyKilled: string[] = []
  const wsDestroyed: string[] = []
  const untracked: string[] = []
  const agentTempId = '_agent_test_temp'

  const deps = {
    ptyManager: {
      spawn: () => ({
        id: 'pty-2',
        pty: {
          onExit: () => undefined,
        },
      }),
      kill: (sessionId: string) => {
        ptyKilled.push(sessionId)
      },
    },
    wsBridge: {
      create: async () => {
        throw new Error('bridge boom')
      },
      destroy: (sessionId: string) => {
        wsDestroyed.push(sessionId)
      },
    },
    workspaceStore: {
      getByPath: () => undefined,
    },
    preferencesStore: {
      get: () => null,
    },
    toolRegistry: {
      get: () => ({ id: 'claude', name: 'Claude', command: 'claude', args: [] }),
      resolveCommand: () => '/usr/bin/claude',
    },
    agentSessionManager: {
      isAgentTool: () => true,
      createSession: async () => ({
        settingsArgs: ['--settings', 'foo'],
        settingsEnv: { CANOPY_TEST: '1' },
        hookPort: 12345,
        hookPath: '/hook/path',
        hookAuthToken: 'token-abc',
        tempId: agentTempId,
      }),
      destroySession: (sessionId: string) => {
        destroyed.push(sessionId)
      },
      getResumeArgs: () => [],
      getCliArgs: () => [],
      getEnvVars: () => ({}),
      rekey: () => undefined,
    },
    windowManager: {
      trackPtySession: () => undefined,
      untrackPtySession: (_windowId: number, sessionId: string) => {
        untracked.push(sessionId)
      },
    },
    tmuxManager: {
      isAvailable: async () => false,
    },
    profileStore: {
      getInternal: async () => undefined,
    },
    getSenderWindow: () =>
      ({
        isDestroyed: () => false,
      }) as unknown as Parameters<
        NonNullable<ToolSpawnEffectDeps['getSenderWindow']>
      >[0] extends never
        ? never
        : ReturnType<NonNullable<ToolSpawnEffectDeps['getSenderWindow']>>,
  } as unknown as ToolSpawnEffectDeps

  const exit = await Effect.runPromiseExit(
    createToolSpawnEffect(deps, createIpcEvent(), {
      toolId: 'claude',
      worktreePath: '/tmp/canopy-worktree',
    }),
  )

  assert.equal(Exit.isFailure(exit), true)

  // Agent session was acquired then must be destroyed exactly once.
  assert.equal(destroyed.length, 1, 'destroySession must be called exactly once')

  // The destroyed id must match what was acquired (pre-rekey: tempId; post-rekey: pty id).
  // After successful pty spawn the program rekeys to session.id ('pty-2').
  // The finalizer must use the most recent id known to the resource.
  assert.equal(destroyed[0], 'pty-2')

  // Pty also cleaned up.
  assert.deepEqual(ptyKilled, ['pty-2'])

  // ws bridge never fully created — destroy should not run because acquire threw.
  assert.deepEqual(wsDestroyed, [])

  // Window tracking was never acquired (failure happened before tracking).
  assert.deepEqual(untracked, [])
})
