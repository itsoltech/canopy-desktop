import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { PairAttemptContext, PairMessage, PairResponse } from './SignalingServer'
import type { PairingUrlInfo } from './types'

const hostWebContents = vi.hoisted(() => ({
  send: vi.fn(),
  isDestroyed: () => false,
  getBackgroundThrottling: () => true,
  setBackgroundThrottling: vi.fn(),
}))

vi.mock('electron', () => ({
  app: {},
  BrowserWindow: { getAllWindows: () => [] },
  powerSaveBlocker: { start: () => 1, stop: () => true, isStarted: () => false },
  webContents: { fromId: () => hostWebContents },
}))

const { RemoteSessionService } = await import('./RemoteSessionService')

// The pairing handshake normally starts from a bound SignalingServer; these
// tests drive the server callbacks directly, so they reach the private state
// the cold-start path would have set up.
interface ServiceInternals {
  hostWcId: number | null
  pendingToken: string | null
  currentPairing: PairingUrlInfo | null
  handlePairAttempt(msg: PairMessage, context: PairAttemptContext): PairResponse
  handlePeerSignal(msg: unknown): void
}

const TOKEN = 'a'.repeat(64)
const OFFER = { type: 'offer', sdp: { type: 'offer', sdp: 'v=0' } }

function fakePreferences(): PreferencesStore {
  const values = new Map<string, string>([['remote.enabled', 'true']])
  return {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => void values.set(key, value),
    delete: (key: string) => void values.delete(key),
  } as unknown as PreferencesStore
}

function sessionAwaitingAccept(): {
  service: InstanceType<typeof RemoteSessionService>
  internals: ServiceInternals
} {
  const service = new RemoteSessionService(fakePreferences())
  const internals = service as unknown as ServiceInternals
  internals.hostWcId = 7
  internals.pendingToken = TOKEN
  internals.currentPairing = {
    pairingUrl: 'http://192.168.1.2:4000/#t=x',
    hostname: 'host',
    lanIp: '192.168.1.2',
    port: 4000,
    expiresAt: Date.now() + 60_000,
  }
  const response = internals.handlePairAttempt(
    { type: 'pair', token: TOKEN, deviceName: 'Phone', deviceId: 'device-1' },
    { localAddress: null },
  )
  expect(response.ok).toBe(true)
  expect(service.getStatus().kind).toBe('peerArrived')
  return { service, internals }
}

function forwardedSignals(): unknown[] {
  return hostWebContents.send.mock.calls
    .filter(([channel]) => channel === 'remote:signal')
    .map(([, msg]) => msg)
}

describe('RemoteSessionService peer signaling gate', () => {
  beforeEach(() => {
    hostWebContents.send.mockClear()
    vi.spyOn(console, 'log').mockImplementation(() => {})
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('drops peer signals while the device still awaits the desktop accept prompt', () => {
    const { service, internals } = sessionAwaitingAccept()

    internals.handlePeerSignal(OFFER)

    expect(forwardedSignals()).toEqual([])
    service.dispose()
  })

  it('forwards peer signals to the host renderer once the device is accepted', async () => {
    const { service, internals } = sessionAwaitingAccept()
    expect((await service.acceptPendingDevice(false)).isOk()).toBe(true)

    internals.handlePeerSignal(OFFER)

    expect(forwardedSignals()).toEqual([OFFER])
    service.dispose()
  })

  it('drops peer signals after the desktop user rejects the device', async () => {
    const { service, internals } = sessionAwaitingAccept()
    expect((await service.rejectPendingDevice()).isOk()).toBe(true)

    internals.handlePeerSignal(OFFER)

    expect(forwardedSignals()).toEqual([])
    service.dispose()
  })
})
