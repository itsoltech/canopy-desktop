import type { SavedInstance } from '../storage/saved-instances-types'
import { PeerController, type PeerPhase } from './PeerController'
import type { RemoteApi } from './RemoteApi'
import { loadOrCreateDeviceId } from './device-id'
import { defaultDeviceName } from './device-name'
import { resetMirrorState } from './mirror-state'

/**
 * Module-level singleton that owns the live `PeerController` for the
 * currently-selected instance. Mobile screens interact with it via
 * `useRemoteSession(instanceId)` — the store survives navigation between
 * instance detail ↔ terminal so the RTC connection isn't torn down on
 * every tab switch.
 *
 * State transitions mirror `PeerController`'s phase machine but with a
 * flatter, UI-friendly shape: callers don't need to know about the
 * difference between `connecting-signaling` and `awaiting-paired`.
 */

export type SessionState =
  | { kind: 'idle' }
  | {
      kind: 'connecting'
      instanceId: string
      phase: 'signaling' | 'pairing' | 'awaiting-accept' | 'rtc'
    }
  | { kind: 'ready'; instanceId: string; api: RemoteApi }
  | { kind: 'error'; instanceId: string; message: string }
  | { kind: 'disconnected'; instanceId: string }

let state: SessionState = { kind: 'idle' }
let controller: PeerController | null = null
let currentInstanceId: string | null = null
const listeners = new Set<(s: SessionState) => void>()

function setState(next: SessionState): void {
  state = next
  for (const l of listeners) l(next)
}

function phaseToState(phase: PeerPhase, instanceId: string, api: RemoteApi | null): SessionState {
  switch (phase.kind) {
    case 'init':
    case 'connecting-signaling':
      return { kind: 'connecting', instanceId, phase: 'signaling' }
    case 'awaiting-paired':
      return { kind: 'connecting', instanceId, phase: 'pairing' }
    case 'awaiting-accept':
      return { kind: 'connecting', instanceId, phase: 'awaiting-accept' }
    case 'negotiating':
      return { kind: 'connecting', instanceId, phase: 'rtc' }
    case 'connected':
      return api
        ? { kind: 'ready', instanceId, api }
        : { kind: 'connecting', instanceId, phase: 'rtc' }
    case 'rejected':
      return { kind: 'error', instanceId, message: `Pairing rejected: ${phase.reason}` }
    case 'error':
      return { kind: 'error', instanceId, message: phase.message }
    case 'disconnected':
      return { kind: 'disconnected', instanceId }
  }
}

export async function connect(instance: SavedInstance): Promise<void> {
  // Idempotent: if we're already connected (or connecting) to this instance,
  // leave the existing session alone.
  if (
    currentInstanceId === instance.id &&
    controller &&
    (state.kind === 'connecting' || state.kind === 'ready')
  ) {
    return
  }

  // Switching instance or starting fresh — tear down any existing session.
  await disconnect()

  currentInstanceId = instance.id
  setState({ kind: 'connecting', instanceId: instance.id, phase: 'signaling' })

  const [deviceId, deviceName] = await Promise.all([
    loadOrCreateDeviceId(),
    Promise.resolve(defaultDeviceName()),
  ])

  const pc = new PeerController({
    lanIp: instance.lanIp,
    port: instance.port,
    token: instance.token,
    deviceId,
    deviceName,
  })
  controller = pc

  let latestApi: RemoteApi | null = null
  pc.onApiReady = (api) => {
    latestApi = api
  }
  pc.onPhaseChange = (phase) => {
    if (currentInstanceId !== instance.id) return
    setState(phaseToState(phase, instance.id, latestApi ?? pc.remoteApi))
  }

  pc.start()
}

export async function disconnect(): Promise<void> {
  if (controller) {
    try {
      controller.dispose()
    } catch {
      /* ignore */
    }
    controller = null
  }
  resetMirrorState()
  currentInstanceId = null
  if (state.kind !== 'idle') {
    setState({ kind: 'idle' })
  }
}

export function getSessionState(): SessionState {
  return state
}

export function getSessionApi(): RemoteApi | null {
  if (state.kind === 'ready') return state.api
  return null
}

export function subscribeSession(listener: (s: SessionState) => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}
