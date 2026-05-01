import assert from 'node:assert/strict'
import test from 'node:test'
import type { IpcMainInvokeEvent } from 'electron'
import { Effect, Exit } from 'effect'
import { createToolSpawnEffect, type ToolSpawnEffectDeps } from './handlers'

function createShellSpawnDeps(overrides: Partial<ToolSpawnEffectDeps> = {}): ToolSpawnEffectDeps {
  const killed: string[] = []
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
      newSession: async () => undefined,
      killSession: async () => undefined,
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
  const exit = await Effect.runPromiseExit(
    createToolSpawnEffect(deps, createIpcEvent(), {
      toolId: 'shell',
      worktreePath: '/tmp/canopy-worktree',
    }),
  )

  assert.equal(Exit.isFailure(exit), true)
  assert.deepEqual((deps.ptyManager as unknown as { __killed: string[] }).__killed, ['pty-1'])
  assert.deepEqual((deps.windowManager as unknown as { __untracked: string[] }).__untracked, [])
})
