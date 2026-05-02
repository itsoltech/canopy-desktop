import assert from 'node:assert/strict'
import test from 'node:test'
import { Effect } from 'effect'
import type { IDisposable, IPty, IPtyForkOptions } from 'node-pty'
import type { WorktreeSetupAction, WorktreeSetupProgress } from '../db/types'
import {
  createWorktreeSetupEffect,
  type SetupContext,
  type WorktreeSetupDeps,
} from './WorktreeSetupRunner'

interface FakePty extends IPty {
  emitData: (chunk: string) => void
  emitExit: (code: number) => void
  killCalls: number
  dataDisposed: boolean
  exitDisposed: boolean
}

function makeFakePty(): FakePty {
  let dataCb: ((data: string) => void) | undefined
  let exitCb: ((evt: { exitCode: number; signal?: number }) => void) | undefined
  let dataDisposed = false
  let exitDisposed = false
  let killCalls = 0

  const fake = {
    pid: 1234,
    cols: 80,
    rows: 24,
    process: 'fake',
    handleFlowControl: false,
    onData: (cb: (data: string) => void): IDisposable => {
      dataCb = cb
      return {
        dispose: () => {
          dataDisposed = true
        },
      }
    },
    onExit: (cb: (evt: { exitCode: number; signal?: number }) => void): IDisposable => {
      exitCb = cb
      return {
        dispose: () => {
          exitDisposed = true
        },
      }
    },
    write: () => undefined,
    resize: () => undefined,
    kill: () => {
      killCalls += 1
    },
    pause: () => undefined,
    resume: () => undefined,
    clear: () => undefined,
    emitData: (chunk: string) => dataCb?.(chunk),
    emitExit: (code: number) => exitCb?.({ exitCode: code }),
  } as unknown as FakePty

  Object.defineProperties(fake, {
    killCalls: { get: () => killCalls },
    dataDisposed: { get: () => dataDisposed },
    exitDisposed: { get: () => exitDisposed },
  })
  return fake
}

interface FakeTimer {
  handle: object
  fire: () => void
  cleared: boolean
}

function makeContext(): SetupContext {
  return {
    repoRoot: '/repo',
    mainWorktreePath: '/repo/main',
    newWorktreePath: '/repo/feature',
  }
}

interface DepsHarness {
  deps: WorktreeSetupDeps
  pty: FakePty | null
  spawnArgs: Array<{ shellPath: string; shellArgs: string[]; opts: IPtyForkOptions }>
  copyCalls: Array<{ src: string; dest: string }>
  mkdirCalls: Array<{ path: string; opts: unknown }>
  copyResults: Array<Promise<void>>
  mkdirResults: Array<Promise<string | undefined>>
  timers: FakeTimer[]
}

interface DepsOptions {
  copyImpls?: Array<() => Promise<void>>
  mkdirImpls?: Array<() => Promise<string | undefined>>
  ptyFactory?: () => FakePty
}

function makeDeps(options: DepsOptions = {}): DepsHarness {
  const harness: DepsHarness = {
    deps: undefined as unknown as WorktreeSetupDeps,
    pty: null,
    spawnArgs: [],
    copyCalls: [],
    mkdirCalls: [],
    copyResults: [],
    mkdirResults: [],
    timers: [],
  }

  const copyImpls = options.copyImpls ?? []
  const mkdirImpls = options.mkdirImpls ?? []
  let copyIdx = 0
  let mkdirIdx = 0

  harness.deps = {
    spawnPty: ((shellPath: string, shellArgs: string[], opts: IPtyForkOptions) => {
      harness.spawnArgs.push({ shellPath, shellArgs, opts })
      const p = options.ptyFactory ? options.ptyFactory() : makeFakePty()
      harness.pty = p
      return p
    }) as unknown as WorktreeSetupDeps['spawnPty'],
    mkdir: ((path: string, opts: unknown) => {
      harness.mkdirCalls.push({ path, opts })
      const impl = mkdirImpls[mkdirIdx] ?? (async () => undefined)
      mkdirIdx += 1
      const result = impl()
      harness.mkdirResults.push(result)
      return result
    }) as unknown as WorktreeSetupDeps['mkdir'],
    copyFile: ((src: string, dest: string) => {
      harness.copyCalls.push({ src, dest })
      const impl = copyImpls[copyIdx] ?? (async () => undefined)
      copyIdx += 1
      const result = impl()
      harness.copyResults.push(result)
      return result
    }) as unknown as WorktreeSetupDeps['copyFile'],
    getLoginEnv: () => ({ SHELL: '/bin/sh', PATH: '/usr/bin' }),
    setTimeout: (handler: () => void) => {
      const handle = {}
      const timer: FakeTimer = {
        handle,
        cleared: false,
        fire: () => {
          if (!timer.cleared) handler()
        },
      }
      harness.timers.push(timer)
      return handle
    },
    clearTimeout: (handle: unknown) => {
      const timer = harness.timers.find((t) => t.handle === handle)
      if (timer) timer.cleared = true
    },
  }

  return harness
}

test('worktree setup copy action reports done progress and returns success', async () => {
  const harness = makeDeps()
  const progress: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [{ type: 'copy', source: '.env', dest: '.env' }]

  const result = await Effect.runPromise(
    createWorktreeSetupEffect(harness.deps, actions, makeContext(), (p) => progress.push(p)),
  )

  assert.deepEqual(result, { success: true, errors: [] })
  // mkdir called for parent of dest, copyFile called for source → dest
  assert.equal(harness.copyCalls.length, 1)
  assert.equal(harness.copyCalls[0].src, '/repo/main/.env')
  assert.equal(harness.copyCalls[0].dest, '/repo/feature/.env')
  // Progress: running, then done
  assert.equal(progress.length, 2)
  assert.equal(progress[0].status, 'running')
  assert.equal(progress[1].status, 'done')
})

test('copy action failure is collected and following actions still run', async () => {
  const harness = makeDeps({
    copyImpls: [
      async () => {
        throw new Error('disk full')
      },
      async () => undefined,
    ],
  })
  const progress: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [
    { type: 'copy', source: 'a.txt', label: 'first' },
    { type: 'copy', source: 'b.txt', label: 'second' },
  ]

  const result = await Effect.runPromise(
    createWorktreeSetupEffect(harness.deps, actions, makeContext(), (p) => progress.push(p)),
  )

  assert.equal(result.success, false)
  assert.equal(result.errors.length, 1)
  assert.match(result.errors[0], /first.*disk full/)

  // Both actions ran
  assert.equal(harness.copyCalls.length, 2)
  // Progress includes error for first, done for second
  const errorEntry = progress.find((p) => p.status === 'error')
  assert.ok(errorEntry, 'expected an error progress entry')
  assert.equal(errorEntry!.label, 'first')
  const doneEntry = progress.find((p) => p.status === 'done')
  assert.ok(doneEntry, 'expected a done progress entry')
  assert.equal(doneEntry!.label, 'second')
})

test('command non-zero exit is collected as command error', async () => {
  const harness = makeDeps()
  const progress: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [{ type: 'command', command: 'echo hi', label: 'ec' }]

  const promise = Effect.runPromise(
    createWorktreeSetupEffect(harness.deps, actions, makeContext(), (p) => progress.push(p)),
  )

  // Allow the Effect.async to register handlers
  await new Promise((r) => setImmediate(r))
  assert.ok(harness.pty, 'pty should be spawned')
  harness.pty!.emitExit(1)

  const result = await promise
  assert.equal(result.success, false)
  assert.equal(result.errors.length, 1)
  assert.match(result.errors[0], /ec.*exited with code 1/)
})

test('command timeout kills pty and clears listeners', async () => {
  const harness = makeDeps()
  const progress: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [{ type: 'command', command: 'sleep 100', label: 'sl' }]

  const promise = Effect.runPromise(
    createWorktreeSetupEffect(harness.deps, actions, makeContext(), (p) => progress.push(p)),
  )

  await new Promise((r) => setImmediate(r))
  assert.ok(harness.pty)
  assert.equal(harness.timers.length, 1, 'timeout timer should be set')

  // Trigger the timeout manually rather than waiting 5 minutes
  harness.timers[0].fire()

  const result = await promise
  assert.equal(result.success, false)
  assert.match(result.errors[0], /timed out/)

  assert.equal(harness.pty!.killCalls, 1, 'pty.kill should be invoked exactly once on timeout')
  assert.equal(harness.pty!.dataDisposed, true)
  assert.equal(harness.pty!.exitDisposed, true)
})

test('abort before action returns setup aborted without starting pty', async () => {
  const harness = makeDeps()
  const progress: WorktreeSetupProgress[] = []
  const controller = new AbortController()
  controller.abort()

  const actions: WorktreeSetupAction[] = [{ type: 'command', command: 'echo hi' }]

  const result = await Effect.runPromise(
    createWorktreeSetupEffect(
      harness.deps,
      actions,
      makeContext(),
      (p) => progress.push(p),
      controller.signal,
    ),
  )

  assert.deepEqual(result, { success: false, errors: ['Setup aborted'] })
  assert.equal(harness.spawnArgs.length, 0, 'no pty should be spawned after abort')
  assert.equal(harness.copyCalls.length, 0, 'no copy should occur after abort')
  assert.equal(progress.length, 0, 'no progress callbacks before first action')
})

test('registration throwing after pty.spawn cleans up pty and disposables (no leak)', async () => {
  // Simulate a deps.setTimeout that throws synchronously after pty.spawn.
  // This is the classic partial-acquisition leak: pty is alive, but the
  // subsequent registration step fails. The runner must kill the pty.
  const harness = makeDeps()
  const baseDeps = harness.deps
  let setTimeoutCalls = 0
  const failingDeps: WorktreeSetupDeps = {
    ...baseDeps,
    setTimeout: () => {
      setTimeoutCalls += 1
      throw new Error('timer scheduling failed')
    },
  }

  const progress: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [{ type: 'command', command: 'echo hi', label: 'rg' }]

  const result = await Effect.runPromise(
    createWorktreeSetupEffect(failingDeps, actions, makeContext(), (p) => progress.push(p)),
  )

  assert.equal(setTimeoutCalls, 1, 'setTimeout should have been called (and thrown)')
  assert.equal(result.success, false)
  assert.equal(result.errors.length, 1)
  assert.match(result.errors[0], /rg.*timer scheduling failed/)

  // Critical: the pty was spawned then registration failed — it MUST be
  // killed so we do not leak the underlying child process.
  assert.ok(harness.pty, 'pty should have been spawned before the throw')
  assert.equal(harness.pty!.killCalls, 1, 'spawned pty must be killed when registration throws')

  // Listeners that managed to register before the throw must be disposed
  // (in this scenario setTimeout throws before onData/onExit, so disposable
  // tracking should remain false — but cleanup must not itself throw).
  assert.equal(harness.pty!.dataDisposed, false)
  assert.equal(harness.pty!.exitDisposed, false)
})

test('registration throwing in p.onExit cleans up pty and the data disposable', async () => {
  // onData has been registered, then onExit throws. The runner must dispose
  // the data listener AND kill the pty.
  const harness = makeDeps({
    ptyFactory: () => {
      const base = makeFakePty()
      // Override onExit to throw synchronously.
      ;(base as unknown as { onExit: unknown }).onExit = () => {
        throw new Error('onExit registration failed')
      }
      return base
    },
  })

  const progress: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [{ type: 'command', command: 'echo hi', label: 'rg2' }]

  const result = await Effect.runPromise(
    createWorktreeSetupEffect(harness.deps, actions, makeContext(), (p) => progress.push(p)),
  )

  assert.equal(result.success, false)
  assert.match(result.errors[0], /rg2.*onExit registration failed/)

  assert.ok(harness.pty)
  assert.equal(harness.pty!.killCalls, 1, 'pty must be killed after partial registration')
  assert.equal(
    harness.pty!.dataDisposed,
    true,
    'data disposable acquired before throw must be disposed',
  )
  // The timeout must also have been cleared.
  assert.equal(harness.timers.length, 1)
  assert.equal(
    harness.timers[0].cleared,
    true,
    'timeout must be cleared on partial-registration cleanup',
  )
})

test('throwing running/done progress callbacks do not fail successful flow', async () => {
  // running and done get thrown from. The overall result must still be
  // success and the typed errors array stays empty.
  const harness = makeDeps()
  const seen: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [{ type: 'command', command: 'echo hi', label: 'rd' }]

  const promise = Effect.runPromise(
    createWorktreeSetupEffect(harness.deps, actions, makeContext(), (p) => {
      seen.push(p)
      if (p.status === 'running' && !p.outputChunk) throw new Error('running boom')
      if (p.status === 'done') throw new Error('done boom')
    }),
  )

  await new Promise((r) => setImmediate(r))
  assert.ok(harness.pty)
  harness.pty!.emitExit(0)

  const result = await promise
  assert.deepEqual(result, { success: true, errors: [] })

  // running was emitted (and threw), then done was emitted (and threw).
  assert.ok(seen.find((p) => p.status === 'running' && !p.outputChunk))
  assert.ok(seen.find((p) => p.status === 'done'))
})

test('throwing error progress callback does not mask the underlying typed failure', async () => {
  // A failing copy action should be collected into errors[] regardless of
  // the error progress callback throwing.
  const harness = makeDeps({
    copyImpls: [
      async () => {
        throw new Error('copy boom')
      },
    ],
  })

  const seen: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [
    { type: 'copy', source: 'x.txt', dest: 'x.txt', label: 'cp' },
  ]

  const result = await Effect.runPromise(
    createWorktreeSetupEffect(harness.deps, actions, makeContext(), (p) => {
      seen.push(p)
      if (p.status === 'error') throw new Error('error progress boom')
    }),
  )

  // The typed failure must be collected into errors[] even though the
  // error-status progress callback threw.
  assert.equal(result.success, false)
  assert.equal(result.errors.length, 1)
  assert.match(result.errors[0], /cp.*copy boom/)

  // The error progress event was fired (and we swallowed its throw).
  assert.ok(
    seen.find((p) => p.status === 'error' && p.label === 'cp'),
    'expected the error progress event to be emitted',
  )
})

test('progress callback throwing on output chunk does not fail the command', async () => {
  const harness = makeDeps()
  let throwOnce = true
  const progress: WorktreeSetupProgress[] = []
  const actions: WorktreeSetupAction[] = [{ type: 'command', command: 'echo hi', label: 'ec' }]

  const promise = Effect.runPromise(
    createWorktreeSetupEffect(harness.deps, actions, makeContext(), (p) => {
      if (p.outputChunk && throwOnce) {
        throwOnce = false
        throw new Error('progress boom')
      }
      progress.push(p)
    }),
  )

  await new Promise((r) => setImmediate(r))
  assert.ok(harness.pty)
  harness.pty!.emitData('chunk-1')
  harness.pty!.emitExit(0)

  const result = await promise
  assert.deepEqual(result, { success: true, errors: [] })
  // running + done; progress chunk that threw was swallowed but not collected.
  assert.equal(
    progress.some((p) => p.status === 'done'),
    true,
  )
})
