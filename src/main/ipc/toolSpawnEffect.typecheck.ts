import type { IpcMainInvokeEvent } from 'electron'
import { Effect } from 'effect'
import { createToolSpawnEffect, type ToolSpawnPayload, type ToolSpawnResult } from './handlers'

declare const event: IpcMainInvokeEvent
declare const payload: ToolSpawnPayload

declare const dependencies: Parameters<typeof createToolSpawnEffect>[0]

const program = createToolSpawnEffect(dependencies, event, payload)

const assertEffect: Effect.Effect<ToolSpawnResult, unknown, never> = program
void assertEffect

// @ts-expect-error - payload requires a tool id and worktree path.
createToolSpawnEffect(dependencies, event, { toolId: 'shell' })
