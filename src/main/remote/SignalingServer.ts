import http from 'node:http'
import { timingSafeEqual } from 'node:crypto'
import { WebSocketServer } from 'ws'
import type { WebSocket as WsWebSocket } from 'ws'
import { ResultAsync, errAsync, okAsync } from 'neverthrow'
import { fromExternalCall, errorMessage } from '../errors'
import type { RemoteServerError } from './errors'
import { RemoteClientHost } from './RemoteClientHost'
import type { HostSignal } from '../../renderer-shared/remote/signalingProtocol'

const MAX_MESSAGE_BYTES = 256 * 1024 // 256 KB — SDP offers can be a few KB; signaling messages are tiny

/**
 * How long a freshly-opened WebSocket has to send its `pair` message before we
 * forcibly close it. Because this server binds on 0.0.0.0, any LAN peer can
 * open a WebSocket; without a handshake deadline those unpaired sockets would
 * accumulate and exhaust the server's connection slots.
 */
const PAIR_HANDSHAKE_TIMEOUT_MS = 15_000

/**
 * Callbacks the SignalingServer fires upward into RemoteSessionService.
 *
 * Kept as a small interface so the server doesn't import the orchestrator
 * (avoids a circular dep) and so tests can drive the server with stubs.
 */
export interface SignalingServerHandlers {
  /**
   * Called when a peer's first WS message claims a token. Should validate the
   * token (constant-time), enforce single-peer policy, and return whether to
   * accept the WS or close it.
   */
  onPairAttempt: (msg: PairMessage) => PairResponse
  /**
   * Called for every signaling message *after* the pair handshake. The server
   * has already verified that this WS is the active peer; the payload is
   * expected to be one of the non-`pair` variants of `PeerSignal`
   * (see `renderer-shared/remote/signalingProtocol`), but is typed as
   * `unknown` because it came over the wire — the orchestrator forwards it
   * to the renderer which is responsible for validation.
   */
  onPeerSignal: (msg: unknown) => void
  /**
   * Called when the active peer disconnects (close or error). The orchestrator
   * decides whether to enter the reconnect window or fully tear down.
   */
  onPeerDisconnected: () => void
}

export interface PairMessage {
  type: 'pair'
  token: string
  deviceName?: string
  deviceId?: string
  publicKeyJwk?: unknown
}

export type PairResponse =
  | {
      ok: true
      sessionId: string
      /**
       * When `true`, the SignalingServer should immediately send an
       * `accepted` frame right after the `paired` response. Used by the
       * trusted-device auto-accept path so the peer can start its WebRTC
       * offer without a manual modal click. The frame is sent AFTER the
       * active-peer slot is assigned to this WebSocket — if we tried to
       * send it from inside `onPairAttempt`, `sendToPeer` would fire into
       * a stale or null `activePeer` because the slot isn't set until
       * after the handler returns.
       */
      autoAccept?: boolean
    }
  | { ok: false; reason: string }

/**
 * Local HTTP + WebSocket server that:
 *
 *   1. Hosts the built remote-client SPA at `GET /remote/*` (via {@link RemoteClientHost})
 *   2. Accepts a single WebSocket peer at `WS /signaling` and forwards
 *      pairing + WebRTC SDP/ICE messages to the orchestrator
 *
 * **Bind address.** Unlike `WsBridge` and `AgentHookServer` (both `127.0.0.1`),
 * this server binds on `0.0.0.0` so a phone on the same WiFi can reach it via
 * the host's LAN address. This is an *intentional* widening of the listening
 * surface — the server is only started while the user explicitly has a remote
 * session open, and only accepts WS peers that present a valid one-shot token.
 *
 * Lazy lifecycle: nothing is bound until {@link start} is called, and
 * {@link stop} releases the port. Re-using a single instance across multiple
 * sessions is safe — `start` after `stop` works.
 */
export class SignalingServer {
  private server: http.Server | null = null
  private wss: WebSocketServer | null = null
  private port = 0
  private bundleHost: RemoteClientHost | null = null
  private activePeer: WsWebSocket | null = null
  private handlers: SignalingServerHandlers | null = null

  get isRunning(): boolean {
    return this.server !== null
  }

  get listeningPort(): number {
    return this.port
  }

  start(opts: {
    bundleRoot: string
    handlers: SignalingServerHandlers
    /**
     * Preferred port to bind to. If the port is free, the server listens on
     * it; if it's in use (or fails for any other reason) we fall back to an
     * ephemeral port (`0`). Omitting this or passing `0` goes straight to an
     * ephemeral port. Passing a stable preferred port lets the peer client's
     * localStorage (device ID, trusted-device flag) survive restarts because
     * the peer origin (`http://ip:PORT`) stays the same.
     */
    preferredPort?: number
  }): ResultAsync<{ port: number }, RemoteServerError> {
    if (this.server) {
      return errAsync({ _tag: 'AlreadyRunning' })
    }

    this.bundleHost = new RemoteClientHost(opts.bundleRoot)
    this.handlers = opts.handlers
    const preferredPort = opts.preferredPort && opts.preferredPort > 0 ? opts.preferredPort : 0

    return fromExternalCall(
      (async () => {
        // First attempt: try the preferred port (may be 0 = ephemeral).
        // On EADDRINUSE we throw away this server and create a fresh one
        // for the ephemeral fallback — reusing the same `http.Server`
        // instance after a failed `listen()` is unreliable across Node
        // versions, especially within a TIME_WAIT window after a prior
        // shutdown. A fresh instance per attempt is the only thing we've
        // found to always work.
        try {
          return await this.attemptListen(preferredPort)
        } catch (e) {
          const isAddrInUse =
            typeof e === 'object' &&
            e !== null &&
            'code' in e &&
            (e as NodeJS.ErrnoException).code === 'EADDRINUSE'
          if (!isAddrInUse || preferredPort === 0) {
            throw e
          }
          console.warn(`[remote] preferred port ${preferredPort} in use, falling back to ephemeral`)
          // Give the kernel a tick to release any half-closed state, then
          // retry on an ephemeral port with a brand-new server instance.
          await new Promise((resolve) => setTimeout(resolve, 50))
          return await this.attemptListen(0)
        }
      })(),
      (e): RemoteServerError => ({ _tag: 'PortBindFailed', message: errorMessage(e) }),
    ).map(({ server, wss, port }) => {
      this.server = server
      this.wss = wss
      this.port = port
      return { port }
    })
  }

  /**
   * Create a fresh HTTP + WebSocket server pair and try to bind them to
   * `port` on `0.0.0.0`. Returns the bound server/wss on success and
   * rejects with the underlying error (tagged with `code` for EADDRINUSE
   * classification) on failure. The caller decides whether to retry.
   *
   * Creates a NEW server per call rather than reusing `this.server` so a
   * failed attempt can't leave us with a half-initialized `http.Server`
   * that behaves strangely on the next `listen()`.
   */
  private attemptListen(
    port: number,
  ): Promise<{ server: http.Server; wss: WebSocketServer; port: number }> {
    return new Promise((resolve, reject) => {
      const server = http.createServer((req, res) => this.handleHttpRequest(req, res))
      const wss = new WebSocketServer({ server, path: '/signaling' })
      wss.on('connection', (ws) => this.handleWsConnection(ws))

      const cleanup = (): void => {
        try {
          wss.close()
        } catch {
          /* ignore */
        }
        try {
          server.close()
        } catch {
          /* ignore */
        }
      }
      const onError = (e: Error): void => {
        server.removeListener('listening', onListening)
        cleanup()
        reject(e)
      }
      const onListening = (): void => {
        server.removeListener('error', onError)
        const addr = server.address()
        const bound = typeof addr === 'object' && addr ? addr.port : 0
        if (bound === 0) {
          cleanup()
          reject(new Error('server.address() returned no port'))
          return
        }
        resolve({ server, wss, port: bound })
      }
      server.once('error', onError)
      server.once('listening', onListening)
      // INTENTIONAL: bind on 0.0.0.0 so LAN peers can reach this. Other
      // local servers in this codebase (WsBridge, AgentHookServer) bind
      // on 127.0.0.1; this one is different on purpose. Only listens
      // while a remote session is active.
      server.listen(port, '0.0.0.0')
    })
  }

  /**
   * Send a server → peer message on the active WebSocket. No-op if there is
   * no peer yet (e.g. before pairing handshake) or if the socket is closing.
   *
   * Accepts either a fully-typed {@link HostSignal} (handshake messages
   * emitted by `RemoteSessionService`) or any JSON-serializable object so the
   * desktop renderer can forward SDP/ICE frames verbatim after they come
   * through `remote:sendSignal` as `unknown`.
   */
  sendToPeer(msg: HostSignal | Record<string, unknown>): void {
    const ws = this.activePeer
    if (!ws || ws.readyState !== ws.OPEN) return
    ws.send(JSON.stringify(msg))
  }

  /**
   * Force-close the active peer (used by orchestrator on idle timeout, manual
   * disconnect, or when promoting a different device to active).
   */
  closePeer(reason?: string): void {
    const ws = this.activePeer
    if (!ws) return
    if (reason && ws.readyState === ws.OPEN) {
      try {
        ws.send(JSON.stringify({ type: 'bye', reason }))
      } catch {
        /* ignore */
      }
    }
    ws.close()
    this.activePeer = null
  }

  stop(): ResultAsync<void, RemoteServerError> {
    if (!this.server) return okAsync(undefined)
    this.closePeer('server stopping')

    const server = this.server
    const wss = this.wss
    this.server = null
    this.wss = null
    this.port = 0
    this.bundleHost = null
    this.handlers = null

    return fromExternalCall(
      new Promise<void>((resolve) => {
        if (wss) wss.close()
        let done = false
        const finish = (): void => {
          if (done) return
          done = true
          resolve()
        }
        // Force-close any idle keep-alive connections so server.close() can
        // finish — otherwise stop() could hang indefinitely on a lingering
        // HTTP(S) connection, leaving the port bound and the next start()
        // failing with EADDRINUSE.
        server.close(finish)
        server.closeAllConnections?.()
        setTimeout(() => {
          if (!done) {
            try {
              server.closeAllConnections?.()
            } catch {
              /* ignore */
            }
            finish()
          }
        }, 5_000)
      }),
      (): RemoteServerError => ({ _tag: 'NotRunning' }),
    )
  }

  private handleHttpRequest(req: http.IncomingMessage, res: http.ServerResponse): void {
    // Health endpoint (used for diagnostics)
    if (req.url === '/health') {
      res.writeHead(200, { 'Content-Type': 'text/plain; charset=utf-8' })
      res.end('ok')
      return
    }

    // Root → redirect to /remote/ for convenience when typing the URL by hand
    if (req.url === '/' || req.url === '') {
      res.writeHead(302, { Location: '/remote/' })
      res.end()
      return
    }

    if (this.bundleHost?.handleRequest(req, res)) return

    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' })
    res.end('Not found')
  }

  private handleWsConnection(ws: WsWebSocket): void {
    let paired = false

    // Boot sockets that never complete the pair handshake — otherwise a LAN
    // peer could open an arbitrary number of idle WebSockets.
    const pairTimer = setTimeout(() => {
      if (!paired) ws.close(1008, 'pair timeout')
    }, PAIR_HANDSHAKE_TIMEOUT_MS)

    ws.on('message', (raw) => {
      const buf = typeof raw === 'string' ? Buffer.from(raw) : (raw as Buffer)
      // Per-message size limit. Each signaling frame (SDP, ICE candidate,
      // pair handshake) is well under 256KB on its own; anything larger
      // is abusive and we boot the peer with 1009 (Policy Violation).
      // This is a per-frame check, NOT a cumulative lifetime cap — a
      // long-lived session may legitimately move many MB of data across
      // many small frames.
      if (buf.byteLength > MAX_MESSAGE_BYTES) {
        ws.close(1009, 'message too large')
        return
      }

      let parsed: unknown
      try {
        parsed = JSON.parse(buf.toString('utf-8'))
      } catch {
        // Drop malformed frames silently
        return
      }

      if (!paired) {
        // First message MUST be a pair attempt
        if (!isPairMessage(parsed)) {
          ws.close(1008, 'expected pair message first')
          return
        }
        if (!this.handlers) {
          ws.close(1011, 'server not ready')
          return
        }
        const response = this.handlers.onPairAttempt(parsed)
        if (!response.ok) {
          try {
            ws.send(JSON.stringify({ type: 'rejected', reason: response.reason }))
          } catch {
            /* ignore */
          }
          ws.close(1008, response.reason)
          return
        }
        // Promote this WS to be the active peer (closing any prior one)
        if (this.activePeer && this.activePeer !== ws) {
          try {
            this.activePeer.close(1000, 'replaced by new peer')
          } catch {
            /* ignore */
          }
        }
        this.activePeer = ws
        paired = true
        clearTimeout(pairTimer)
        try {
          ws.send(JSON.stringify({ type: 'paired', sessionId: response.sessionId }))
        } catch {
          /* ignore */
        }
        // Trusted-device auto-accept: send `accepted` right after `paired`
        // so the peer can skip awaiting-accept and go straight into
        // WebRTC negotiation. This must happen HERE (post-activePeer
        // assignment) rather than inside onPairAttempt — otherwise
        // sendToPeer would have fired with `this.activePeer` still
        // pointing at the previous (possibly null) socket.
        if (response.autoAccept) {
          try {
            ws.send(JSON.stringify({ type: 'accepted' }))
          } catch {
            /* ignore */
          }
        }
        return
      }

      // Already paired — forward to orchestrator. The orchestrator validates
      // structure (offer/answer/ice/bye) and either replies via sendToPeer or
      // forwards to the desktop renderer's RemoteHostController.
      this.handlers?.onPeerSignal(parsed)
    })

    const onClose = (): void => {
      clearTimeout(pairTimer)
      if (this.activePeer === ws) {
        this.activePeer = null
        this.handlers?.onPeerDisconnected()
      }
    }
    ws.on('close', onClose)
    ws.on('error', onClose)
  }
}

function isPairMessage(value: unknown): value is PairMessage {
  if (typeof value !== 'object' || value === null) return false
  const v = value as Record<string, unknown>
  return v.type === 'pair' && typeof v.token === 'string' && v.token.length > 0
}

/**
 * Constant-time token comparison helper exported for use by RemoteSessionService.
 * Tokens are random hex of equal length, so length mismatch is also a mismatch.
 */
export function tokensMatch(provided: string, expected: string): boolean {
  if (provided.length !== expected.length) return false
  try {
    return timingSafeEqual(Buffer.from(provided), Buffer.from(expected))
  } catch {
    return false
  }
}
