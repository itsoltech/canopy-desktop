import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { okAsync, type ResultAsync } from 'neverthrow'

vi.mock('electron', () => ({
  app: { getAppPath: () => '/app' },
  BrowserWindow: { getAllWindows: () => [] },
  powerSaveBlocker: { start: () => 1, stop: () => undefined, isStarted: () => false },
  webContents: { fromId: () => null },
}))

const signaling = vi.hoisted(() => ({
  handlers: null as null | {
    onPairAttempt: (msg: unknown, context: unknown) => { ok: boolean }
  },
  closePeer: vi.fn(),
}))

vi.mock('./SignalingServer', () => ({
  tokensMatch: (a: string, b: string) => a === b,
  SignalingServer: class {
    isRunning = false
    listeningHost: string | null = null
    listeningPort = 0
    start(opts: {
      bindHost: string
      handlers: typeof signaling.handlers
    }): ResultAsync<{ port: number }, never> {
      this.isRunning = true
      this.listeningHost = opts.bindHost
      this.listeningPort = 4123
      signaling.handlers = opts.handlers
      return okAsync({ port: 4123 })
    }
    stop(): ResultAsync<void, never> {
      this.isRunning = false
      return okAsync(undefined)
    }
    closePeer(reason?: string): void {
      signaling.closePeer(reason)
    }
    sendToPeer(): void {
      // Frames to the peer are not observed by these tests.
    }
  },
}))

vi.mock('./discovery', () => ({
  selectPrimaryInterface: () => ({ name: 'en0', address: '192.168.1.10' }),
}))

import { RemoteSessionService } from './RemoteSessionService'

const PHONE = 'a1b2c3d4e5f60718293a4b5c6d7e8f90'
const IDLE_TIMEOUT_MS = 15 * 60 * 1000

function trustedPrefs(): { get(key: string): string | null; set(k: string, v: string): void } {
  const prefs = new Map<string, string>([
    ['remote.enabled', 'true'],
    [
      'remote.trustedDevices',
      JSON.stringify([
        { deviceId: PHONE, publicKeyJwk: null, name: 'Phone', addedAt: '', lastSeen: '' },
      ]),
    ],
  ])
  return {
    get: (key) => prefs.get(key) ?? null,
    set: (key, value) => void prefs.set(key, value),
  }
}

async function pairTrustedPhone(): Promise<RemoteSessionService> {
  const service = new RemoteSessionService(trustedPrefs() as never)
  await service.start(1, 'en0')
  const response = signaling.handlers!.onPairAttempt(
    { type: 'pair', token: 'stale-token', deviceId: PHONE },
    { localAddress: '192.168.1.10' },
  )
  expect(response.ok).toBe(true)
  expect(service.getStatus().kind).toBe('paired')
  return service
}

describe('RemoteSessionService', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    signaling.closePeer.mockClear()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('ends the live session of a trusted device when it is removed', async () => {
    const service = await pairTrustedPhone()

    service.removeTrustedDevice(PHONE)
    await vi.advanceTimersByTimeAsync(0)

    expect(signaling.closePeer).toHaveBeenCalled()
    expect(service.getStatus().kind).not.toBe('paired')
    const retry = signaling.handlers!.onPairAttempt(
      { type: 'pair', token: 'stale-token', deviceId: PHONE },
      { localAddress: '192.168.1.10' },
    )
    expect(retry.ok).toBe(false)
  })

  it('starts the idle timeout when the device is accepted', async () => {
    const service = await pairTrustedPhone()

    await vi.advanceTimersByTimeAsync(IDLE_TIMEOUT_MS + 1)

    expect(service.getStatus().kind).not.toBe('paired')
  })

  it('keeps a session open while activity keeps resetting the idle timeout', async () => {
    const service = await pairTrustedPhone()

    await vi.advanceTimersByTimeAsync(IDLE_TIMEOUT_MS - 60_000)
    service.resetIdleTimer()
    await vi.advanceTimersByTimeAsync(IDLE_TIMEOUT_MS - 60_000)

    expect(service.getStatus().kind).toBe('paired')
  })
})
