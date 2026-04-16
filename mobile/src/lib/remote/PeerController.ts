// Ported from src/renderer/src/remote/lib/peer/PeerController.ts
// Adaptations for React Native:
//   - RTCPeerConnection imported from react-native-webrtc (not the global)
//   - Pairing URL already parsed into `SavedInstance` upstream — the
//     controller receives host/port/token directly, no window.location
//   - Device ID supplied by caller (async load via SecureStore happens in
//     session.ts before instantiation)
//   - Device name supplied by caller (from expo-device via device-name.ts)

import { RTCPeerConnection } from 'react-native-webrtc'

import { DataChannelRpc } from './DataChannelRpc'
import { createRemoteApi, type RemoteApi } from './RemoteApi'
import { SignalingClient } from './SignalingClient'
import { StateApplier } from './StateApplier'
import { ICE_SERVERS } from './protocol/ice-config'
import { CHANNEL_COMMANDS, CHANNEL_STATE, CHANNEL_STREAM } from './protocol/rpc-protocol'
import type { HostSignal, PeerSignal } from './protocol/signaling'

/**
 * Peer (remote client) side of the WebRTC handshake. Drives the full flow
 * from "session connect" to "data channels open":
 *
 *   1. Open a WebSocket to `ws://<lanIp>:<port>/signaling`
 *   2. Send the `pair` message with the token and device identity
 *   3. Wait for `paired` (token OK) then `accepted` (user approved on host)
 *   4. Create an `RTCPeerConnection`, build the three data channels
 *      (commands/state/stream), build an SDP offer and send it via signaling
 *   5. Apply the returned answer, trickle ICE candidates both ways
 *   6. Once channels open, surface `connected` to the UI
 *
 * Once the `commands` channel opens, we wrap it in a {@link DataChannelRpc}
 * engine and build a typed {@link RemoteApi} facade. In parallel the
 * {@link StateApplier} starts and hydrates the mirror store with the host's
 * projects/tabs/tools snapshot plus delta subscriptions.
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

export interface PeerControllerOptions {
  lanIp: string
  port: number
  token: string
  deviceId: string
  deviceName: string
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
  private opts: PeerControllerOptions
  private openChannels = 0
  private commandsRpc: DataChannelRpc | null = null
  private api: RemoteApi | null = null
  private stateApplier: StateApplier | null = null

  /** UI hook — `session.ts` listens to this to translate to SessionState. */
  onPhaseChange: ((phase: PeerPhase) => void) | null = null

  /**
   * Fires once the commands channel is open and the typed API facade is
   * ready to use.
   */
  onApiReady: ((api: RemoteApi) => void) | null = null

  constructor(opts: PeerControllerOptions) {
    this.opts = opts
  }

  get currentPhase(): PeerPhase {
    return this.phase
  }

  get remoteApi(): RemoteApi | null {
    return this.api
  }

  start(): void {
    if (this.disposed) return
    this.setPhase({ kind: 'connecting-signaling' })

    // Plain ws://. HTTPS/wss:// is a future-work toggle.
    const url = `ws://${this.opts.lanIp}:${this.opts.port}/signaling`

    this.signaling.onOpen = () => {
      this.setPhase({ kind: 'awaiting-paired' })
      const pair: PeerSignal = {
        type: 'pair',
        token: this.opts.token,
        deviceName: this.opts.deviceName,
        deviceId: this.opts.deviceId,
      }
      this.signaling.send(pair)
    }
    this.signaling.onMessage = (msg) => this.handleSignal(msg)
    this.signaling.onError = () => {
      if (this.phase.kind === 'error' || this.phase.kind === 'disconnected') return
      this.setPhase({ kind: 'error', message: 'WebSocket error' })
    }
    this.signaling.onClose = (code, reason) => {
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
    switch (msg.type) {
      case 'paired':
        if (this.phase.kind === 'awaiting-paired') {
          this.setPhase({ kind: 'awaiting-accept' })
        }
        return
      case 'rejected':
        this.setPhase({ kind: 'rejected', reason: msg.reason })
        this.signaling.close(1000, 'rejected')
        return
      case 'accepted':
        if (this.phase.kind === 'awaiting-accept') {
          this.setPhase({ kind: 'negotiating' })
          await this.startOffer()
        }
        return
      case 'answer':
        if (!this.pc) return
        try {
          // react-native-webrtc's setRemoteDescription expects sdp: string
          // (not string|undefined). The wire type allows undefined to match
          // the W3C init dict; the host always populates it.
          await (
            this.pc as unknown as {
              setRemoteDescription: (d: { type: string; sdp: string }) => Promise<void>
            }
          ).setRemoteDescription({
            type: msg.sdp.type ?? 'answer',
            sdp: msg.sdp.sdp ?? '',
          })
        } catch (e) {
          this.setPhase({
            kind: 'error',
            message: `Failed to apply answer: ${(e as Error).message}`,
          })
        }
        return
      case 'ice':
        if (!this.pc) return
        try {
          await (
            this.pc as unknown as {
              addIceCandidate: (c: RTCIceCandidateInit) => Promise<void>
            }
          ).addIceCandidate(msg.candidate)
        } catch (e) {
          console.warn('[peer] addIceCandidate failed:', e)
        }
        return
      case 'offer':
        console.warn('[peer] unexpected offer from host')
        return
      case 'bye':
        this.setPhase({ kind: 'disconnected' })
        this.dispose()
        return
      default:
        return
    }
  }

  private async startOffer(): Promise<void> {
    const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS })
    this.pc = pc

    // react-native-webrtc exposes the W3C API on the instance (with on*
    // property setters). We use the same shape the desktop peer uses.
    ;(pc as unknown as { onicecandidate: (e: RTCPeerConnectionIceEvent) => void }).onicecandidate =
      (e) => {
        if (this.disposed) return
        if (!e.candidate) return
        this.signaling.send({ type: 'ice', candidate: e.candidate.toJSON() })
      }
    ;(pc as unknown as { onconnectionstatechange: () => void }).onconnectionstatechange = () => {
      const s = (pc as unknown as { connectionState: string }).connectionState
      if (s === 'failed' || s === 'closed') {
        if (this.phase.kind === 'connected' || this.phase.kind === 'negotiating') {
          this.setPhase({ kind: 'disconnected' })
        }
      }
    }

    // Data channels MUST be created on the offerer side *before* createOffer,
    // otherwise the answerer never sees them in the SDP.
    const createChannel = (label: string, init: RTCDataChannelInit): RTCDataChannel =>
      (
        pc as unknown as {
          createDataChannel: (l: string, i: RTCDataChannelInit) => RTCDataChannel
        }
      ).createDataChannel(label, init)

    this.channels.commands = createChannel(CHANNEL_COMMANDS, { ordered: true })
    this.channels.state = createChannel(CHANNEL_STATE, { ordered: true })
    this.channels.stream = createChannel(CHANNEL_STREAM, {
      ordered: true,
      maxRetransmits: 0,
    })

    for (const ch of Object.values(this.channels)) {
      if (!ch) continue
      this.wireChannel(ch)
    }

    try {
      const offer = await pc.createOffer({})
      await pc.setLocalDescription(offer)
      this.signaling.send({
        type: 'offer',
        sdp: offer as unknown as RTCSessionDescriptionInit,
      })
    } catch (e) {
      this.setPhase({
        kind: 'error',
        message: `Failed to create offer: ${(e as Error).message}`,
      })
    }
  }

  private wireChannel(ch: RTCDataChannel): void {
    const channel = ch as unknown as {
      label: string
      onopen: (() => void) | null
      onclose: (() => void) | null
      onerror: ((e: Event) => void) | null
      onmessage: ((e: MessageEvent) => void) | null
    }
    channel.onopen = () => {
      this.openChannels += 1
      if (channel.label === CHANNEL_COMMANDS) {
        this.attachRpcToCommandsChannel(ch)
      }
      if (this.openChannels === 3 && this.phase.kind === 'negotiating') {
        this.setPhase({ kind: 'connected' })
      }
    }
    channel.onclose = () => {
      this.openChannels = Math.max(0, this.openChannels - 1)
      if (channel.label === CHANNEL_COMMANDS) {
        this.commandsRpc?.dispose()
        this.commandsRpc = null
        this.api = null
      }
    }
    channel.onerror = (e) => console.warn(`[peer] channel error ${channel.label}`, e)
    // The `commands` channel is owned by `DataChannelRpc` via addEventListener.
    // Non-RPC channels log as placeholders for now.
    if (channel.label !== CHANNEL_COMMANDS) {
      channel.onmessage = (e) => {
        console.log(`[peer] ${channel.label} msg:`, e.data)
      }
    }
  }

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
