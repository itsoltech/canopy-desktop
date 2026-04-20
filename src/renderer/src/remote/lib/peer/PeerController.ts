import { match } from 'ts-pattern'
import { ICE_SERVERS } from '../../../../../renderer-shared/remote/iceConfig'
import {
  CHANNEL_COMMANDS,
  CHANNEL_STATE,
  CHANNEL_STREAM,
} from '../../../../../renderer-shared/rpc/protocol'
import { DataChannelRpc } from '../../../../../renderer-shared/rpc/DataChannelRpc'
import type {
  HostSignal,
  PeerSignal,
} from '../../../../../renderer-shared/remote/signalingProtocol'
import { SignalingClient } from './SignalingClient'
import { createRemoteApi, type RemoteApi } from '../api'
import { StateApplier } from '../state/StateApplier'

/**
 * Peer (remote client) side of the WebRTC handshake. Drives the full flow
 * from QR scan to "data channels open":
 *
 *   1. Parse the pairing URL fragment (`#t=...&h=...`) for token + host label
 *   2. Open a WebSocket to the signaling endpoint
 *   3. Send the `pair` message with the token and a generated device identity
 *   4. Wait for `paired` (token OK) then `accepted` (user approved on host)
 *   5. Create an `RTCPeerConnection`, build the three data channels
 *      (commands/state/stream), build an SDP offer and send it via signaling
 *   6. Apply the returned answer, trickle ICE candidates both ways
 *   7. Once channels open, surface `connected` to the UI
 *
 * Once the `commands` channel opens, we wrap it in a {@link DataChannelRpc}
 * engine and build a typed {@link RemoteApi} facade
 * (`remoteApi.diag.ping(n)`), which the Svelte UI uses to talk to the host.
 * In parallel the {@link StateApplier} starts and hydrates the mirror store
 * with the host's projects/tabs/tools snapshot plus delta subscriptions.
 */

export type PeerPhase =
  | { kind: 'init' }
  | { kind: 'connecting-signaling' }
  | { kind: 'awaiting-paired' }
  | { kind: 'awaiting-accept' }
  | { kind: 'negotiating' }
  | { kind: 'connected' }
  | { kind: 'rejected'; reason: string }
  | { kind: 'disconnected' }
  | { kind: 'error'; message: string }

export interface PairingUrlParts {
  token: string
  host: string
  port: number
  hostname: string
}

/**
 * Parse `window.location` (or a supplied URL) into the four pieces the peer
 * controller needs. The token + hostname live in the URL fragment so they
 * aren't sent in any HTTP request.
 */
export function parsePairingLocation(loc: Location): PairingUrlParts | null {
  const hash = loc.hash.startsWith('#') ? loc.hash.slice(1) : loc.hash
  if (!hash) return null
  const params = new URLSearchParams(hash)
  const token = params.get('t')
  const hostname = params.get('h') ?? 'host'
  if (!token) return null
  const port = Number(loc.port) || (loc.protocol === 'https:' ? 443 : 80)
  return {
    token,
    host: loc.hostname,
    port,
    hostname,
  }
}

function randomHex(bytes: number): string {
  const arr = new Uint8Array(bytes)
  crypto.getRandomValues(arr)
  return Array.from(arr, (b) => b.toString(16).padStart(2, '0')).join('')
}

/**
 * Load (or mint on first run) a stable device identifier from
 * `localStorage`. The host uses this ID to recognize the same device
 * across reconnects — if the user clicks "Remember this device" on the
 * host, subsequent pair attempts from the same browser profile skip the
 * accept modal and go straight to `paired`.
 *
 * Without this, the device ID was regenerated every time the
 * `PeerController` was constructed (which is once per page load), so
 * the host's trusted-device store never matched and the "remember me"
 * button silently did nothing on refresh.
 */
const DEVICE_ID_STORAGE_KEY = 'canopy.remote.deviceId'

export function loadOrCreateDeviceId(): string {
  if (typeof window === 'undefined' || !window.localStorage) {
    return randomHex(16)
  }
  try {
    const existing = window.localStorage.getItem(DEVICE_ID_STORAGE_KEY)
    if (existing && /^[0-9a-f]+$/i.test(existing) && existing.length >= 16) {
      return existing
    }
    const fresh = randomHex(16)
    window.localStorage.setItem(DEVICE_ID_STORAGE_KEY, fresh)
    return fresh
  } catch {
    // localStorage may throw in private mode on some browsers. Fall back
    // to an ephemeral ID — we lose trusted-device identity for this
    // session but the pairing flow still works.
    return randomHex(16)
  }
}

function defaultDeviceName(): string {
  const ua = navigator.userAgent
  if (/iPhone/i.test(ua)) return 'iPhone'
  if (/iPad/i.test(ua)) return 'iPad'
  if (/Android/i.test(ua)) return 'Android device'
  if (/Macintosh/i.test(ua)) return 'Mac'
  if (/Windows/i.test(ua)) return 'Windows PC'
  if (/Linux/i.test(ua)) return 'Linux device'
  return 'Remote browser'
}

export class PeerController {
  private pc: RTCPeerConnection | null = null
  private signaling = new SignalingClient()
  private channels: {
    commands?: RTCDataChannel
    state?: RTCDataChannel
    stream?: RTCDataChannel
  } = {}
  private phase: PeerPhase = { kind: 'init' }
  private disposed = false
  private parts: PairingUrlParts | null = null
  private deviceId: string
  private openChannels = 0
  private commandsRpc: DataChannelRpc | null = null
  private api: RemoteApi | null = null
  private stateApplier: StateApplier | null = null

  /** UI hook — RemoteApp.svelte listens to this to render its state. */
  onPhaseChange: ((phase: PeerPhase) => void) | null = null

  /**
   * Fires once the commands channel is open and the typed API facade is
   * ready to use. The RemoteApp.svelte UI wires this to kick off its
   * diagnostic ping loop.
   */
  onApiReady: ((api: RemoteApi) => void) | null = null

  constructor(opts?: { deviceId?: string }) {
    // Prefer an explicitly-supplied ID (used by tests), then the one
    // persisted in localStorage, then a fresh random as final fallback.
    this.deviceId = opts?.deviceId ?? loadOrCreateDeviceId()
  }

  get currentPhase(): PeerPhase {
    return this.phase
  }

  get remoteApi(): RemoteApi | null {
    return this.api
  }

  start(loc: Location): void {
    if (this.disposed) return
    const parts = parsePairingLocation(loc)
    if (!parts) {
      this.setPhase({ kind: 'error', message: 'Missing or invalid pairing token in URL fragment' })
      return
    }
    this.parts = parts
    this.setPhase({ kind: 'connecting-signaling' })

    // Plain ws://. HTTPS fallback flips to wss:// when the host enables it
    // in settings. The signaling endpoint lives at /signaling.
    const url = `ws://${parts.host}:${parts.port}/signaling`

    this.signaling.onOpen = () => {
      this.setPhase({ kind: 'awaiting-paired' })
      const pair: PeerSignal = {
        type: 'pair',
        token: parts.token,
        deviceName: defaultDeviceName(),
        deviceId: this.deviceId,
      }
      this.signaling.send(pair)
    }
    this.signaling.onMessage = (msg) => this.handleSignal(msg)
    this.signaling.onError = () => {
      if (this.phase.kind === 'error' || this.phase.kind === 'disconnected') return
      this.setPhase({ kind: 'error', message: 'WebSocket error' })
    }
    this.signaling.onClose = (code, reason) => {
      // If we already transitioned to a terminal state (rejected/error) keep
      // that more informative phase; otherwise surface a generic disconnect.
      if (
        this.phase.kind === 'rejected' ||
        this.phase.kind === 'error' ||
        this.phase.kind === 'disconnected'
      ) {
        return
      }
      this.setPhase({ kind: 'disconnected' })
      console.log('[peer] signaling closed:', code, reason)
    }

    this.signaling.connect(url)
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.stateApplier?.dispose()
    this.stateApplier = null
    this.commandsRpc?.dispose()
    this.commandsRpc = null
    this.api = null
    try {
      this.signaling.send({ type: 'bye' })
    } catch {
      /* ignore */
    }
    this.signaling.close()
    for (const ch of Object.values(this.channels)) {
      try {
        ch?.close()
      } catch {
        /* ignore */
      }
    }
    this.channels = {}
    if (this.pc) {
      try {
        this.pc.close()
      } catch {
        /* ignore */
      }
      this.pc = null
    }
  }

  // ===== signaling handlers =====

  private async handleSignal(msg: HostSignal): Promise<void> {
    if (this.disposed) return
    await match(msg)
      .with({ type: 'paired' }, () => {
        // Token was accepted, now we wait for the human to approve on host.
        if (this.phase.kind === 'awaiting-paired') {
          this.setPhase({ kind: 'awaiting-accept' })
        }
      })
      .with({ type: 'rejected' }, (m) => {
        this.setPhase({ kind: 'rejected', reason: m.reason })
        this.signaling.close(1000, 'rejected')
      })
      .with({ type: 'accepted' }, async () => {
        if (this.phase.kind === 'awaiting-accept') {
          this.setPhase({ kind: 'negotiating' })
          await this.startOffer()
        }
      })
      .with({ type: 'answer' }, async (m) => {
        if (!this.pc) return
        try {
          await this.pc.setRemoteDescription(m.sdp)
        } catch (e) {
          this.setPhase({
            kind: 'error',
            message: `Failed to apply answer: ${(e as Error).message}`,
          })
        }
      })
      .with({ type: 'ice' }, async (m) => {
        if (!this.pc) return
        try {
          await this.pc.addIceCandidate(m.candidate)
        } catch (e) {
          console.warn('[peer] addIceCandidate failed:', e)
        }
      })
      .with({ type: 'offer' }, () => {
        // We are the offerer — receiving an offer from the host is a
        // protocol error in the current flow. Renegotiation may revisit this.
        console.warn('[peer] unexpected offer from host')
      })
      .with({ type: 'bye' }, () => {
        this.setPhase({ kind: 'disconnected' })
        this.dispose()
      })
      .exhaustive()
  }

  private async startOffer(): Promise<void> {
    const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS })
    this.pc = pc

    pc.onicecandidate = (e) => {
      if (this.disposed) return
      if (!e.candidate) return
      this.signaling.send({ type: 'ice', candidate: e.candidate.toJSON() })
    }
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'failed' || pc.connectionState === 'closed') {
        if (this.phase.kind === 'connected' || this.phase.kind === 'negotiating') {
          this.setPhase({ kind: 'disconnected' })
        }
      }
    }

    // Data channels MUST be created on the offerer side *before* createOffer,
    // otherwise the answerer never sees them in the SDP.
    this.channels.commands = pc.createDataChannel(CHANNEL_COMMANDS, { ordered: true })
    this.channels.state = pc.createDataChannel(CHANNEL_STATE, { ordered: true })
    this.channels.stream = pc.createDataChannel(CHANNEL_STREAM, {
      ordered: true,
      maxRetransmits: 0,
    })

    for (const ch of Object.values(this.channels)) {
      if (!ch) continue
      this.wireChannel(ch)
    }

    try {
      const offer = await pc.createOffer()
      await pc.setLocalDescription(offer)
      this.signaling.send({ type: 'offer', sdp: offer })
    } catch (e) {
      this.setPhase({
        kind: 'error',
        message: `Failed to create offer: ${(e as Error).message}`,
      })
    }
  }

  private wireChannel(ch: RTCDataChannel): void {
    ch.onopen = () => {
      this.openChannels += 1
      if (ch.label === CHANNEL_COMMANDS) {
        this.attachRpcToCommandsChannel(ch)
      }
      if (this.openChannels === 3 && this.phase.kind === 'negotiating') {
        this.setPhase({ kind: 'connected' })
      }
    }
    ch.onclose = () => {
      this.openChannels = Math.max(0, this.openChannels - 1)
      if (ch.label === CHANNEL_COMMANDS) {
        this.commandsRpc?.dispose()
        this.commandsRpc = null
        this.api = null
      }
    }
    ch.onerror = (e) => console.warn(`[peer] channel error ${ch.label}`, e)
    // The `commands` channel is owned by `DataChannelRpc` (which attaches its
    // own listener via addEventListener), so we only install a raw
    // `onmessage` handler for the non-RPC channels that still log as
    // placeholders until they get structured payloads.
    if (ch.label !== CHANNEL_COMMANDS) {
      ch.onmessage = (e) => {
        console.log(`[peer] ${ch.label} msg:`, e.data)
      }
    }
  }

  /**
   * Wire the newly opened commands channel into a typed RPC client and
   * publish the resulting {@link RemoteApi} to whoever is listening on
   * `onApiReady`. Also hydrate the mirror store via {@link StateApplier},
   * which pulls `state.getSnapshot` and then subscribes to delta topics on
   * the same RPC so the mirror stays in sync with the host in real time.
   */
  private attachRpcToCommandsChannel(ch: RTCDataChannel): void {
    if (this.commandsRpc || this.disposed) return
    const rpc = new DataChannelRpc(ch, { label: 'peer.commands' })
    this.commandsRpc = rpc
    this.api = createRemoteApi(rpc)
    this.stateApplier = new StateApplier()
    void this.stateApplier.start(this.api)
    this.onApiReady?.(this.api)
  }

  private setPhase(next: PeerPhase): void {
    this.phase = next
    this.onPhaseChange?.(next)
  }
}
