import type { IpcMain, IpcMainInvokeEvent } from 'electron'
import { Effect } from 'effect'
import { handleIpcEffect, runIpcEffect, type IpcEffectFailure } from './effectRunner'

const ipcEvent = {} as IpcMainInvokeEvent

const prefsGetHandler = runIpcEffect('db:prefs:get:typecheck', (_event, payload: { key: string }) =>
  Effect.succeed(payload.key),
)

const prefsGetResult: Promise<string> = prefsGetHandler(ipcEvent, { key: 'theme' })

// @ts-expect-error - Effect-backed IPC handlers keep the channel payload typed.
prefsGetHandler(ipcEvent, { value: 'missing-key' })

const fallbackHandler = runIpcEffect(
  'db:prefs:get:fallback-typecheck',
  () => Effect.fail(new Error('boom')),
  { fallback: (failure: IpcEffectFailure) => `${failure.tag ?? 'untagged'}:${failure.message}` },
)

const fallbackResult: Promise<string> = fallbackHandler(ipcEvent)

// IpcEffectFailure must remain transport-safe: only channel, optional tag,
// and message. Adding `cause`, stack traces, or other internals would leak
// renderer-bound failures.
const _safeFailure: IpcEffectFailure = {
  channel: 'db:prefs:get:shape-check',
  tag: 'BoomError',
  message: 'boom',
}
void _safeFailure

const _failureWithLeakedCause: IpcEffectFailure = {
  channel: 'db:prefs:get:shape-check',
  message: 'boom',
  // @ts-expect-error - cause/stack/instance details must never be exposed over IPC.
  cause: new Error('leaked'),
}
void _failureWithLeakedCause

const _failureWithLegacyTag: IpcEffectFailure = {
  channel: 'db:prefs:get:shape-check',
  message: 'boom',
  // @ts-expect-error - the field is `tag`, not `errorTag`.
  errorTag: 'BoomError',
}
void _failureWithLegacyTag

const registeredHandlers: Array<{
  channel: string
  listener: (event: IpcMainInvokeEvent, payload: { key: string }) => Promise<string>
}> = []

const ipcMain = {
  handle: (
    channel: string,
    listener: (event: IpcMainInvokeEvent, payload: { key: string }) => Promise<string>,
  ) => {
    registeredHandlers.push({ channel, listener })
  },
} as IpcMain

handleIpcEffect(ipcMain, 'db:prefs:get:typecheck-register', (_event, payload: { key: string }) =>
  Effect.succeed(payload.key),
)

void Promise.all([prefsGetResult, fallbackResult])
