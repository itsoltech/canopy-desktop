import assert from 'node:assert/strict'
import test from 'node:test'
import type { IpcMainInvokeEvent } from 'electron'
import { Effect, Exit } from 'effect'
import { createToolSpawnEffect, type ToolSpawnEffectDeps, type ToolSpawnError } from './handlers'

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
  assert.deepEqual((deps.tmuxManager as unknown as { __created: string[] }).__created, [
    'canopy_workspace-1',
  ])
  assert.deepEqual((deps.tmuxManager as unknown as { __killed: string[] }).__killed, [
    'canopy_workspace-1',
  ])
  assert.deepEqual((deps.ptyManager as unknown as { __killed: string[] }).__killed, ['pty-1'])
})
