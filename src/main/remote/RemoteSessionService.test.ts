import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ResultAsync } from 'neverthrow'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { SignalingServerHandlers } from './SignalingServer'
import { RemoteSessionService } from './RemoteSessionService'

const harness = vi.hoisted(() => ({
  hostWebContents: {
    send: vi.fn(),
    isDestroyed: () => false,
    getBackgroundThrottling: () => true,
    setBackgroundThrottling: () => {},
  },
  handlers: null as SignalingServerHandlers | null,
  sentToPeer: [] as unknown[],
}))

vi.mock('electron', () => ({
  app: { getAppPath: () => '/app' },
  BrowserWindow: { getAllWindows: () => [] },
  powerSaveBlocker: { start: () => 1, stop: () => true, isStarted: () => false },
  webContents: { fromId: () => harness.hostWebContents },
}))

vi.mock('./discovery', () => ({
  selectPrimaryInterface: () => ({ name: 'en0', address: '192.168.1.10', virtual: false }),
}))

vi.mock('./SignalingServer', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./SignalingServer')>()
  const { okAsync } = await import('neverthrow')
  class FakeSignalingServer {
    isRunning = false
    listeningHost = '192.168.1.10'
    listeningPort = 0

    start(opts: { handlers: SignalingServerHandlers }): ResultAsync<{ port: number }, never> {
      harness.handlers = opts.handlers
      this.isRunning = true
      this.listeningPort = 4242
      return okAsync({ port: 4242 })
    }

    stop(): ResultAsync<void, never> {
      this.isRunning = false
      return okAsync<void, never>(undefined)
    }

    sendToPeer(msg: unknown): void {
      harness.sentToPeer.push(msg)
    }

    closePeer(): void {
      // No socket to close in the fake.
    }
  }
  return { ...actual, SignalingServer: FakeSignalingServer }
})

const OFFER = { type: 'offer', sdp: { type: 'offer', sdp: 'v=0' } }

function preferences(): PreferencesStore {
  const values = new Map<string, string>([['remote.enabled', 'true']])
  return {
    get: (key: string): string | null => values.get(key) ?? null,
    set: (key: string, value: string): void => void values.set(key, value),
    delete: (key: string): void => void values.delete(key),
  } as unknown as PreferencesStore
}

async function startWithPendingPeer(service: RemoteSessionService): Promise<void> {
  const { pairingUrl } = (await service.start(7, 'en0'))._unsafeUnwrap()
  const token = new URLSearchParams(new URL(pairingUrl).hash.slice(1)).get('t') ?? ''
  const response = harness.handlers?.onPairAttempt(
    { type: 'pair', token, deviceId: 'phone-1', deviceName: 'Phone' },
    { localAddress: null },
  )
  expect(response).toMatchObject({ ok: true })
  expect(service.getStatus().kind).toBe('peerArrived')
}

describe('RemoteSessionService device approval gate', () => {
  let service: RemoteSessionService

  beforeEach(() => {
    harness.hostWebContents.send.mockReset()
    harness.handlers = null
    harness.sentToPeer = []
    service = new RemoteSessionService(preferences())
  })

  afterEach(() => {
    service.dispose()
  })

  it('keeps peer signaling away from the host window until the user accepts the device', async () => {
    await startWithPendingPeer(service)

    harness.handlers?.onPeerSignal(OFFER)
    expect(harness.hostWebContents.send).not.toHaveBeenCalledWith('remote:signal', OFFER)

    ;(await service.acceptPendingDevice(false))._unsafeUnwrap()
    harness.handlers?.onPeerSignal(OFFER)
    expect(harness.hostWebContents.send).toHaveBeenCalledWith('remote:signal', OFFER)
  })

  it('refuses to send host signaling to a peer the user has not accepted', async () => {
    await startWithPendingPeer(service)

    const result = await service.forwardSignalToPeer({ type: 'answer', sdp: { type: 'answer' } })

    expect(result.isErr()).toBe(true)
    expect(harness.sentToPeer).not.toContainEqual(expect.objectContaining({ type: 'answer' }))
  })
})
