import type { IpcMain, IpcMainInvokeEvent } from 'electron'
import { Cause, Effect, Exit, Option } from 'effect'

// IpcEffectFailure is the only shape that ever crosses the IPC boundary or
// reaches a fallback. Keep it narrow: channel + optional discriminant tag +
// human-readable message. Never widen this to include Cause, stack traces,
// class instances, or arbitrary `cause` data — those are not transport-safe.
export interface IpcEffectFailure {
  readonly channel: string
  readonly tag?: string
  readonly message: string
}

export interface IpcEffectOptions<A> {
  fallback?: (failure: IpcEffectFailure) => A
}

function expectedErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error
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

function causeToFailure(channel: string, cause: Cause.Cause<unknown>): IpcEffectFailure {
  // Only expose details for expected (typed) failures. Defects, interrupts,
  // and unknown causes get a generic message so we never leak stack traces or
  // internal class instances over IPC.
  const failure = Cause.failureOption(cause)
  if (Option.isSome(failure)) {
    return {
      channel,
      tag: expectedErrorTag(failure.value),
      message: expectedErrorMessage(failure.value),
    }
  }
  return { channel, message: 'IPC handler failed' }
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
  return async (event, ...args) => {
    const program = Effect.suspend(() => buildEffect(event, ...args))
    const exit = await Effect.runPromiseExit(program)
    if (Exit.isSuccess(exit)) return exit.value

    const failure = causeToFailure(channel, exit.cause)

    console.error(`[ipc:${channel}] handler failed`, failure)

    if (options.fallback) {
      return options.fallback(failure)
    }

    // Reject with a plain Error carrying only the safe message so the
    // serialized rejection on the renderer side never includes Cause data.
    // The primary safety property is that `message` carries no Cause data.
    //
    // We also lock `.stack` to an empty, non-writable, non-configurable value
    // so that nothing (including v8's lazy formatter or any later code that
    // might assign onto the rejection) can re-introduce a main-process stack
    // before Electron's structured-clone serializer reads it. Once frozen
    // here, `Error.prototype.toString` and Electron's serializer both observe
    // the empty string we wrote.
    const rejection = new Error(`[ipc:${channel}] ${failure.message}`)
    Object.defineProperty(rejection, 'stack', {
      value: '',
      writable: false,
      configurable: false,
      enumerable: false,
    })
    throw rejection
  }
}
