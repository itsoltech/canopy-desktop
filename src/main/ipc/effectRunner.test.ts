import assert from 'node:assert/strict'
import { afterEach, beforeEach, test, vi } from 'vitest'
import type { IpcMain, IpcMainInvokeEvent } from 'electron'
import { Data, Effect } from 'effect'
import { handleIpcEffect, runIpcEffect, type IpcEffectFailure } from './effectRunner'

let expectedConsoleErrors = 0
let consoleErrorCalls: unknown[][] = []

// Failure-path tests intentionally trigger effectRunner.ts console.error output
// (that logging is part of its contract). Capture it at the test boundary so
// Vitest output stays readable, but assert the exact expected count/shape so an
// unexpected console.error in a success test cannot be silently hidden.
beforeEach(() => {
  expectedConsoleErrors = 0
  consoleErrorCalls = []
  vi.spyOn(console, 'error').mockImplementation((...args: unknown[]) => {
    consoleErrorCalls.push(args)
  })
})

afterEach(() => {
  try {
    assert.equal(consoleErrorCalls.length, expectedConsoleErrors)
    for (const args of consoleErrorCalls) {
      assert.equal(typeof args[0], 'string')
      assert.match(args[0] as string, /^\[ipc:.+\] handler failed$/)
      payloadIsTransportSafe(args[1])
    }
  } finally {
    vi.restoreAllMocks()
  }
})

function expectIpcFailureLog(count = 1): void {
  expectedConsoleErrors += count
}

class TaggedFailure extends Data.TaggedError('TaggedFailure')<{
  readonly message: string
  readonly cause?: unknown
}> {}

function fakeEvent(): IpcMainInvokeEvent {
  return { sender: { id: 1 } } as unknown as IpcMainInvokeEvent
}

function fakeIpcMain(): {
  handlers: Map<string, (event: IpcMainInvokeEvent, ...args: unknown[]) => Promise<unknown>>
  ipcMain: IpcMain
} {
  const handlers = new Map<
    string,
    (event: IpcMainInvokeEvent, ...args: unknown[]) => Promise<unknown>
  >()
  const ipcMain = {
    handle: (channel: string, listener: (...args: unknown[]) => Promise<unknown>) => {
      handlers.set(
        channel,
        listener as (event: IpcMainInvokeEvent, ...args: unknown[]) => Promise<unknown>,
      )
    },
  } as unknown as IpcMain
  return { handlers, ipcMain }
}

function payloadIsTransportSafe(value: unknown): void {
  // Walk the value and assert no forbidden runtime fields are present.
  const seen = new WeakSet<object>()
  function walk(v: unknown, path: string): void {
    if (v === null || v === undefined) return
    if (typeof v !== 'object') return
    if (seen.has(v as object)) return
    seen.add(v as object)
    if (v instanceof Error) {
      throw new Error(`payload contains Error instance at ${path}`)
    }
    for (const key of Object.keys(v as Record<string, unknown>)) {
      if (key === 'cause' || key === 'stack') {
        throw new Error(`payload contains forbidden key '${key}' at ${path}`)
      }
      walk((v as Record<string, unknown>)[key], `${path}.${key}`)
    }
  }
  walk(value, '$')
}

test('handleIpcEffect returns fallback for expected typed failure without leaking cause', async () => {
  expectIpcFailureLog()
  const { handlers, ipcMain } = fakeIpcMain()
  const captured: IpcEffectFailure[] = []

  handleIpcEffect(
    ipcMain,
    'test:typed-fail',
    () =>
      Effect.fail(
        new TaggedFailure({ message: 'visible message', cause: new Error('SECRET INTERNAL') }),
      ),
    {
      fallback: (failure) => {
        captured.push(failure)
        return { ok: false as const, message: failure.message, tag: failure.tag }
      },
    },
  )

  const handler = handlers.get('test:typed-fail')
  assert.ok(handler, 'handler must be registered')

  const result = (await handler!(fakeEvent())) as {
    ok: false
    message: string
    tag?: string
  }

  // Fallback gets the safe IpcEffectFailure shape only.
  assert.equal(captured.length, 1)
  assert.equal(captured[0].channel, 'test:typed-fail')
  assert.equal(captured[0].tag, 'TaggedFailure')
  assert.equal(captured[0].message, 'visible message')
  // No `cause` should be propagated to the failure descriptor.
  assert.equal('cause' in captured[0], false)
  assert.equal('stack' in captured[0], false)

  // Returned value also stays transport-safe.
  assert.equal(result.ok, false)
  assert.equal(result.message, 'visible message')
  assert.equal(result.tag, 'TaggedFailure')
  payloadIsTransportSafe(result)
  payloadIsTransportSafe(captured[0])
})

test('runIpcEffect throws sanitized message for expected failure', async () => {
  expectIpcFailureLog()
  const handler = runIpcEffect('test:throw', () =>
    Effect.fail(new TaggedFailure({ message: 'boundary fail', cause: 'internal-stack-here' })),
  )

  let caught: unknown
  try {
    await handler(fakeEvent())
    assert.fail('handler should reject')
  } catch (e) {
    caught = e
  }

  assert.ok(caught instanceof Error)
  const err = caught as Error
  assert.equal(err.message, '[ipc:test:throw] boundary fail')
  // No internal cause string was concatenated into the rejection.
  assert.equal(err.message.includes('internal-stack-here'), false)
})

test('handleIpcEffect maps defects to a generic fallback message', async () => {
  expectIpcFailureLog()
  const { handlers, ipcMain } = fakeIpcMain()

  handleIpcEffect(
    ipcMain,
    'test:die',
    () => Effect.die(new Error('SECRET INTERNAL DEFECT')) as Effect.Effect<unknown, never>,
    {
      fallback: (failure) => ({ ok: false as const, message: failure.message }),
    },
  )

  const handler = handlers.get('test:die')
  assert.ok(handler)
  const result = (await handler!(fakeEvent())) as { ok: false; message: string }

  // Generic fallback message, internal defect string must not be exposed.
  assert.equal(result.message, 'IPC handler failed')
  assert.equal(result.message.includes('SECRET INTERNAL DEFECT'), false)
  payloadIsTransportSafe(result)
})

test('runIpcEffect throws generic message for defect without leaking internals', async () => {
  expectIpcFailureLog()
  const handler = runIpcEffect(
    'test:die-throw',
    () => Effect.die(new Error('SECRET DEFECT')) as Effect.Effect<unknown, never>,
  )

  let caught: unknown
  try {
    await handler(fakeEvent())
  } catch (e) {
    caught = e
  }
  assert.ok(caught instanceof Error)
  const err = caught as Error
  assert.equal(err.message, '[ipc:test:die-throw] IPC handler failed')
  assert.equal(err.message.includes('SECRET DEFECT'), false)
})

test('handleIpcEffect success path preserves successful value', async () => {
  const { handlers, ipcMain } = fakeIpcMain()

  handleIpcEffect(ipcMain, 'test:success', () => Effect.succeed({ greeting: 'hello', count: 42 }))

  const handler = handlers.get('test:success')
  assert.ok(handler)
  const result = (await handler!(fakeEvent())) as { greeting: string; count: number }

  assert.deepEqual(result, { greeting: 'hello', count: 42 })
})

test('runIpcEffect rejection has empty stack so main-process frames do not cross IPC', async () => {
  expectIpcFailureLog()
  // The thrown Error's `.stack` is part of Electron's IPC error serialization
  // surface area. Even though the message is sanitized, a v8-generated stack
  // would expose internal main-process file paths to the renderer. We strip
  // it explicitly. This test pins that contract.
  const handler = runIpcEffect('test:stack', () =>
    Effect.fail(new TaggedFailure({ message: 'boundary fail' })),
  )

  let caught: unknown
  try {
    await handler(fakeEvent())
    assert.fail('handler should reject')
  } catch (e) {
    caught = e
  }

  assert.ok(caught instanceof Error)
  const err = caught as Error
  assert.equal(err.message, '[ipc:test:stack] boundary fail')
  // Stack must be empty (or absent) — no internal frames leak through.
  assert.equal(err.stack ?? '', '')
})

test('runIpcEffect rejection .stack is locked so it cannot be re-attached after the fact', async () => {
  expectIpcFailureLog()
  // Defense-in-depth: it is not enough to clear .stack at throw time. Any
  // code that later mutates the rejection (Electron serializer, devtools,
  // a global error listener) could re-attach a stack-like string. The
  // property must be non-writable AND non-configurable so reassignment
  // either throws (strict mode) or silently no-ops, never mutates.
  const handler = runIpcEffect('test:stack-lock', () =>
    Effect.fail(new TaggedFailure({ message: 'boundary fail' })),
  )

  let caught: unknown
  try {
    await handler(fakeEvent())
    assert.fail('handler should reject')
  } catch (e) {
    caught = e
  }

  assert.ok(caught instanceof Error)
  const err = caught as Error
  const desc = Object.getOwnPropertyDescriptor(err, 'stack')
  assert.ok(desc, 'stack descriptor must exist')
  assert.equal(desc!.writable, false)
  assert.equal(desc!.configurable, false)
  assert.equal(desc!.value, '')

  // Direct reassignment must not be able to introduce a stack value.
  let reassignThrew = false
  try {
    ;(err as unknown as { stack: string }).stack = 'src/main/secret.ts:42'
  } catch {
    reassignThrew = true
  }
  // Either strict-mode TypeError or silent no-op — both are acceptable;
  // what matters is the value cannot change.
  assert.equal(err.stack ?? '', '')
  // Smoke check: the throw outcome is allowed but not required.
  void reassignThrew
})

test('runIpcEffect rejection for defect has empty stack and generic message', async () => {
  expectIpcFailureLog()
  const handler = runIpcEffect(
    'test:die-stack',
    () => Effect.die(new Error('SECRET')) as Effect.Effect<unknown, never>,
  )

  let caught: unknown
  try {
    await handler(fakeEvent())
  } catch (e) {
    caught = e
  }
  assert.ok(caught instanceof Error)
  const err = caught as Error
  assert.equal(err.message, '[ipc:test:die-stack] IPC handler failed')
  assert.equal(err.stack ?? '', '')
})

test('runIpcEffect passes args through to the effect builder', async () => {
  let receivedEvent: IpcMainInvokeEvent | null = null
  let receivedArgs: unknown[] = []
  const handler = runIpcEffect('test:args', (event: IpcMainInvokeEvent, a: number, b: string) => {
    receivedEvent = event
    receivedArgs = [a, b]
    return Effect.succeed(`got:${a}:${b}`)
  })

  const evt = fakeEvent()
  const result = await handler(evt, 7, 'name')
  assert.equal(result, 'got:7:name')
  assert.equal(receivedEvent, evt)
  assert.deepEqual(receivedArgs, [7, 'name'])
})
