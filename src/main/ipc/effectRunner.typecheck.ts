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
  { fallback: (failure: IpcEffectFailure) => failure.message },
)

const fallbackResult: Promise<string> = fallbackHandler(ipcEvent)

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
