import type { IpcMainInvokeEvent } from 'electron'
import { Effect } from 'effect'
import { runIpcEffect, type IpcEffectFailure } from './effectRunner'

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

void Promise.all([prefsGetResult, fallbackResult])
