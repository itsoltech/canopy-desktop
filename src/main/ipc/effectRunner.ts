import type { IpcMain, IpcMainInvokeEvent } from 'electron'
import { Effect } from 'effect'

export interface IpcEffectFailure {
  _tag: 'IpcEffectFailure'
  channel: string
  errorTag?: string
  message: string
}

export interface IpcEffectOptions<A> {
  fallback?: (failure: IpcEffectFailure) => A
}

function expectedErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'object' && error !== null && 'message' in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === 'string') return message
  }
  return 'IPC handler failed'
}

function expectedErrorTag(error: unknown): string | undefined {
  if (typeof error === 'object' && error !== null && '_tag' in error) {
    const tag = (error as { _tag?: unknown })._tag
    if (typeof tag === 'string') return tag
  }
  return undefined
}

function toIpcEffectFailure(channel: string, error: unknown): IpcEffectFailure {
  return {
    _tag: 'IpcEffectFailure',
    channel,
    errorTag: expectedErrorTag(error),
    message: expectedErrorMessage(error),
  }
}

export function handleIpcEffect<Args extends unknown[], A, E>(
  ipcMain: IpcMain,
  channel: string,
  buildEffect: (event: IpcMainInvokeEvent, ...args: Args) => Effect.Effect<A, E, never>,
  options: IpcEffectOptions<A> = {},
): void {
  ipcMain.handle(channel, runIpcEffect(channel, buildEffect, options))
}

export function runIpcEffect<Args extends unknown[], A, E>(
  channel: string,
  buildEffect: (event: IpcMainInvokeEvent, ...args: Args) => Effect.Effect<A, E, never>,
  options: IpcEffectOptions<A> = {},
): (event: IpcMainInvokeEvent, ...args: Args) => Promise<A> {
  return (event, ...args) => {
    const program = Effect.suspend(() => buildEffect(event, ...args)).pipe(
      Effect.catchAll((error) => {
        const failure = toIpcEffectFailure(channel, error)

        console.error(`[ipc:${channel}] handler failed`, failure)

        if (options.fallback) {
          return Effect.succeed(options.fallback(failure))
        }

        return Effect.fail(new Error(`[ipc:${channel}] ${failure.message}`))
      }),
    )

    return Effect.runPromise(program)
  }
}
