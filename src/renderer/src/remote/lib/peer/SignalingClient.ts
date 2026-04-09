import type {
  HostSignal,
  PeerSignal,
} from '../../../../../renderer-shared/remote/signalingProtocol'

/**
 * Thin WebSocket wrapper used by `PeerController`. Responsibilities:
 *
 *   - Open a `WebSocket` to `ws://<host>:<port>/signaling`
 *   - Serialize outbound {@link PeerSignal} messages as JSON
 *   - Parse inbound frames, narrow them to {@link HostSignal}, hand off to
 *     the registered `onMessage` handler
 *   - Surface open / close / error as events so the controller can drive
 *     its own state machine
 *
 * The class is deliberately dumb about protocol semantics — `PeerController`
 * owns the pairing handshake and WebRTC handshake, this class just moves bytes.
 */
export class SignalingClient {
  private ws: WebSocket | null = null
  private closed = false

  onOpen: (() => void) | null = null
  onMessage: ((msg: HostSignal) => void) | null = null
  onError: ((err: Event) => void) | null = null
  onClose: ((code: number, reason: string) => void) | null = null

  get isOpen(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN
  }

  connect(url: string): void {
    if (this.ws) return
    diagBanner(`connect: ${url}`, 'info')
    let ws: WebSocket
    try {
      ws = new WebSocket(url)
    } catch (e) {
      // The WebSocket constructor throws synchronously on some failure
      // modes: CSP block, invalid URL, wrong scheme for HTTPS page, etc.
      // Without this try/catch the error bubbles up through
      // `PeerController.start` → `onMount` and the UI stays stuck at
      // `connecting-signaling` with no visible explanation. Surfacing
      // it via `onError` lets the controller flip to an `error` phase
      // and the UI shows what went wrong.
      console.error('[peer-signaling] WebSocket constructor threw:', e)
      diagBanner(`ws-ctor threw: ${e}`, 'err')
      const fakeEvent = new Event('error')
      queueMicrotask(() => this.onError?.(fakeEvent))
      return
    }
    this.ws = ws

    ws.addEventListener('open', () => {
      if (this.closed) return
      diagBanner('ws open', 'ok')
      this.onOpen?.()
    })

    ws.addEventListener('message', (ev) => {
      if (this.closed) return
      const text = typeof ev.data === 'string' ? ev.data : null
      if (text === null) {
        console.warn('[peer-signaling] dropped non-text frame')
        return
      }
      let parsed: unknown
      try {
        parsed = JSON.parse(text)
      } catch {
        console.warn('[peer-signaling] bad JSON frame')
        return
      }
      const msg = narrowHostSignal(parsed)
      if (!msg) {
        console.warn('[peer-signaling] unknown signal shape:', parsed)
        return
      }
      diagBanner(`msg: ${msg.type}`, 'info')
      try {
        this.onMessage?.(msg)
      } catch (e) {
        diagBanner(`onMessage threw: ${e}`, 'err')
        throw e
      }
    })

    ws.addEventListener('error', (ev) => {
      if (this.closed) return
      diagBanner('ws error event', 'err')
      this.onError?.(ev)
    })

    ws.addEventListener('close', (ev) => {
      diagBanner(`ws close code=${ev.code} reason=${ev.reason} closed=${this.closed}`, 'err')
      if (this.closed) return
      this.closed = true
      this.onClose?.(ev.code, ev.reason)
    })
  }

  close(code = 1000, reason = 'client closing'): void {
    diagBanner(
      `signaling.close() called: ${reason} (stack: ${new Error().stack?.split('\n').slice(1, 4).join(' | ') ?? '?'})`,
      'err',
    )
    if (this.closed) return
    this.closed = true
    try {
      this.ws?.close(code, reason)
    } catch {
      /* ignore */
    }
    this.ws = null
  }

  send(msg: PeerSignal): void {
    const ws = this.ws
    if (!ws || ws.readyState !== WebSocket.OPEN) return
    ws.send(JSON.stringify(msg))
  }
}

/** Send a diagnostic event to the host's `/diag` endpoint so peer-side
 *  events show up in the main-process terminal alongside server-side
 *  events. Fire-and-forget — if the network call fails (e.g. peer
 *  can't even reach the server), we still log locally so Safari Web
 *  Inspector captures it. */
function diagBanner(text: string, _kind?: 'info' | 'ok' | 'err'): void {
  void _kind
  console.log('[peer:diag]', text)
  try {
    const base = `${window.location.protocol}//${window.location.host}`
    void fetch(`${base}/diag?msg=${encodeURIComponent(text)}`, {
      method: 'GET',
      cache: 'no-store',
      keepalive: true,
    }).catch(() => {
      /* ignore — diag is best-effort */
    })
  } catch {
    /* ignore */
  }
}

function narrowHostSignal(raw: unknown): HostSignal | null {
  if (typeof raw !== 'object' || raw === null) return null
  const v = raw as {
    type?: unknown
    sessionId?: unknown
    reason?: unknown
    sdp?: unknown
    candidate?: unknown
  }
  switch (v.type) {
    case 'paired':
      if (typeof v.sessionId !== 'string') return null
      return { type: 'paired', sessionId: v.sessionId }
    case 'rejected':
      return { type: 'rejected', reason: typeof v.reason === 'string' ? v.reason : 'rejected' }
    case 'accepted':
      return { type: 'accepted' }
    case 'offer':
      if (!v.sdp) return null
      return { type: 'offer', sdp: v.sdp as RTCSessionDescriptionInit }
    case 'answer':
      if (!v.sdp) return null
      return { type: 'answer', sdp: v.sdp as RTCSessionDescriptionInit }
    case 'ice':
      if (!v.candidate) return null
      return { type: 'ice', candidate: v.candidate as RTCIceCandidateInit }
    case 'bye':
      return { type: 'bye', reason: typeof v.reason === 'string' ? v.reason : undefined }
    default:
      return null
  }
}
