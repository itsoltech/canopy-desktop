import type { IpcMain, IpcMainInvokeEvent } from 'electron'
import { Cause, Effect } from 'effect'

export interface IpcEffectFailure {
  _tag: 'IpcEffectFailure'
  channel: string
  message: string
  cause: Cause.Cause<unknown>
}

export interface IpcEffectOptions<A> {
  fallback?: (failure: IpcEffectFailure) => A
}

export function handleIpcEffect<Args extends unknown[], A>(
  ipcMain: IpcMain,
  channel: string,
  buildEffect: (event: IpcMainInvokeEvent, ...args: Args) => Effect.Effect<A, unknown, never>,
  options: IpcEffectOptions<A> = {},
): void {
  ipcMain.handle(channel, runIpcEffect(channel, buildEffect, options))
}

export function runIpcEffect<Args extends unknown[], A>(
  channel: string,
  buildEffect: (event: IpcMainInvokeEvent, ...args: Args) => Effect.Effect<A, unknown, never>,
  options: IpcEffectOptions<A> = {},
): (event: IpcMainInvokeEvent, ...args: Args) => Promise<A> {
  return (event, ...args) => {
    const program = Effect.suspend(() => buildEffect(event, ...args)).pipe(
      Effect.catchAllCause((cause) => {
        const failure: IpcEffectFailure = {
          _tag: 'IpcEffectFailure',
          channel,
          message: Cause.pretty(cause),
          cause,
        }

        console.error(`[ipc:${channel}] handler failed`, failure.message)

        if (options.fallback) {
          return Effect.succeed(options.fallback(failure))
        }

        return Effect.fail(new Error(`[ipc:${channel}] ${failure.message}`))
      }),
    )

    return Effect.runPromise(program)
  }
}
