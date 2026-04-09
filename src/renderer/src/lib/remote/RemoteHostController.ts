import { match, P } from 'ts-pattern'
import { ICE_SERVERS } from '../../../../renderer-shared/remote/iceConfig'
import {
  CHANNEL_COMMANDS,
  CHANNEL_STATE,
  CHANNEL_STREAM,
} from '../../../../renderer-shared/rpc/protocol'
import { DataChannelRpc } from '../../../../renderer-shared/rpc/DataChannelRpc'
import type {
  InboundSignalForRenderer,
  OutboundSignalFromRenderer,
} from '../../../../renderer-shared/remote/signalingProtocol'
import { HostRpcServer } from './HostRpcServer'

/**
 * Desktop side of the WebRTC peer connection.
 *
 * The remote peer is the offerer: it builds the data channels first, then
 * ships an SDP offer. This controller is the answerer — it waits for the
 * offer, creates an answer, and collects incoming data channels via
 * `ondatachannel`.
 *
 * Lifecycle is owned by `remoteSession.svelte.ts`: an instance is created
 * the first time the session reaches `peerArrived` and disposed when the
 * session returns to `idle`. A fresh instance is built for every reconnect
 * (Phase 10 will optimise this with `restartIce()`).
 *
 * Outbound signals (our answer + ICE candidates) go out through
 * `window.api.remote.sendSignal`, which lands in the main process, which
 * forwards to the peer over the signaling WebSocket.
 *
 * Phase 5 wires the `commands` channel into a {@link DataChannelRpc} engine
 * and registers the {@link HostRpcServer} handlers so typed method calls
 * from the remote peer (starting with `diag.ping`) flow through the standard
 * request/response/event framing. `state` and `stream` channels remain raw
 * until Phases 6 and 8 give them structured payloads.
 */
export class RemoteHostController {
  private pc: RTCPeerConnection | null = null
  private channels: {
    commands?: RTCDataChannel
    state?: RTCDataChannel
    stream?: RTCDataChannel
  } = {}
  private commandsRpc: DataChannelRpc | null = null
  private rpcServer: HostRpcServer | null = null
  /** ICE candidates that arrived before `setRemoteDescription` — buffered and replayed. */
  private pendingIce: RTCIceCandidateInit[] = []
  private disposed = false
  private send: (msg: OutboundSignalFromRenderer) => void

  constructor(send: (msg: OutboundSignalFromRenderer) => void) {
    this.send = send
  }

  initialize(): void {
    if (this.pc || this.disposed) return

    const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS })
    this.pc = pc

    pc.onicecandidate = (e) => {
      // Guard against stale candidates from a disposed pc that hasn't
      // finished shutting down yet. Without this, a pc we just tore
      // down during a reconnect can briefly continue emitting ICE,
      // and those candidates would leak into the new peer's session.
      if (this.disposed) return
      if (!e.candidate) return // gathering complete
      this.send({ type: 'ice', candidate: e.candidate.toJSON() })
    }

    pc.ondatachannel = (e) => {
      if (this.disposed) return
      const ch = e.channel
      this.wireChannel(ch)
    }
  }

  /**
   * Handle a signaling message received from the peer via the main process.
   * Validates and narrows to the renderer-facing subset of the wire protocol.
   */
  async handleSignal(raw: unknown): Promise<void> {
    if (this.disposed) return
    const msg = narrowInboundSignal(raw)
    if (!msg) {
      console.warn('[remote-host] dropped malformed signal:', raw)
      return
    }
    if (!this.pc) this.initialize()
    const pc = this.pc!

    await match(msg)
      .with({ type: 'offer' }, async (m) => {
        await pc.setRemoteDescription(m.sdp)
        // Replay any ICE candidates that arrived before the remote description
        // was set. Trickle ICE frequently delivers candidates out-of-order.
        for (const c of this.pendingIce) {
          try {
            await pc.addIceCandidate(c)
          } catch (err) {
            console.warn('[remote-host] flushed ICE rejected:', err)
          }
        }
        this.pendingIce = []
        const answer = await pc.createAnswer()
        await pc.setLocalDescription(answer)
        this.send({ type: 'answer', sdp: answer })
      })
      .with({ type: 'answer' }, async () => {
        // We are the answerer — an inbound answer from the peer is a protocol
        // error and should never happen in the current flow. Log and ignore.
        console.warn('[remote-host] unexpected answer from peer')
      })
      .with({ type: 'ice' }, async (m) => {
        if (!pc.remoteDescription) {
          this.pendingIce.push(m.candidate)
          return
        }
        try {
          await pc.addIceCandidate(m.candidate)
        } catch (err) {
          console.warn('[remote-host] addIceCandidate failed:', err)
        }
      })
      .with({ type: 'bye' }, () => {
        console.log('[remote-host] peer said bye')
        this.dispose()
      })
      .exhaustive()
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.rpcServer?.dispose()
    this.rpcServer = null
    this.commandsRpc?.dispose()
    this.commandsRpc = null
    for (const ch of Object.values(this.channels)) {
      try {
        ch?.close()
      } catch {
        /* ignore */
      }
    }
    this.channels = {}
    this.pendingIce = []
    if (this.pc) {
      try {
        this.pc.close()
      } catch {
        /* ignore */
      }
      this.pc = null
    }
  }

  private wireChannel(ch: RTCDataChannel): void {
    if (ch.label === CHANNEL_COMMANDS) {
      this.channels.commands = ch
    } else if (ch.label === CHANNEL_STATE) {
      this.channels.state = ch
    } else if (ch.label === CHANNEL_STREAM) {
      this.channels.stream = ch
    } else {
      console.warn('[remote-host] unknown channel label:', ch.label)
      // Keep the reference around so the channel isn't GC'd mid-open, but do
      // not expose it through `channels`.
      return
    }

    ch.onopen = () => {
      if (ch.label === CHANNEL_COMMANDS) {
        this.attachRpcToCommandsChannel(ch)
      }
    }
    ch.onclose = () => {
      if (ch.label === CHANNEL_COMMANDS) {
        this.rpcServer?.dispose()
        this.rpcServer = null
        this.commandsRpc?.dispose()
        this.commandsRpc = null
      }
    }
    ch.onerror = (e) => {
      console.warn(`[remote-host] channel error: ${ch.label}`, e)
    }
    // `DataChannelRpc` installs its own `message` listener on the commands
    // channel via `addEventListener`, so we deliberately do NOT set
    // `ch.onmessage` for that label here (that would shadow the RPC engine).
    // For `state` and `stream` we just log until Phases 6/8 give them real
    // payloads.
    if (ch.label !== CHANNEL_COMMANDS) {
      ch.onmessage = (e) => {
        console.log(`[remote-host] msg on ${ch.label}:`, e.data)
      }
    }
  }

  /**
   * Phase 5: the commands channel becomes the transport for the JSON-RPC
   * engine. Called from `onopen` so the `readyState === 'open'` guard inside
   * `DataChannelRpc.call` / `sendFrame` is trivially satisfied for any
   * traffic that happens during the handler registration itself.
   */
  private attachRpcToCommandsChannel(ch: RTCDataChannel): void {
    if (this.commandsRpc || this.disposed) return
    const rpc = new DataChannelRpc(ch, { label: 'remote-host.commands' })
    const server = new HostRpcServer(rpc)
    server.registerAllHandlers()
    this.commandsRpc = rpc
    this.rpcServer = server
  }
}

/**
 * Runtime narrowing for inbound signals from the peer. Because the payload
 * crossed an IPC boundary it comes in typed as `unknown`; this guard rejects
 * anything that doesn't match the renderer-facing subset.
 */
function narrowInboundSignal(raw: unknown): InboundSignalForRenderer | null {
  if (typeof raw !== 'object' || raw === null) return null
  const v = raw as { type?: unknown; sdp?: unknown; candidate?: unknown; reason?: unknown }
  return match(v)
    .with(
      { type: 'offer', sdp: P.not(P.nullish) },
      (x) => ({ type: 'offer', sdp: x.sdp as RTCSessionDescriptionInit }) as const,
    )
    .with(
      { type: 'answer', sdp: P.not(P.nullish) },
      (x) => ({ type: 'answer', sdp: x.sdp as RTCSessionDescriptionInit }) as const,
    )
    .with(
      { type: 'ice', candidate: P.not(P.nullish) },
      (x) => ({ type: 'ice', candidate: x.candidate as RTCIceCandidateInit }) as const,
    )
    .with(
      { type: 'bye' },
      (x) =>
        ({
          type: 'bye',
          reason: typeof x.reason === 'string' ? x.reason : undefined,
        }) as const,
    )
    .otherwise(() => null)
}
