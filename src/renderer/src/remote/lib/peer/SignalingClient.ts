import { match } from 'ts-pattern'
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
      const fakeEvent = new Event('error')
      queueMicrotask(() => this.onError?.(fakeEvent))
      return
    }
    this.ws = ws

    ws.addEventListener('open', () => {
      if (this.closed) return
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
      this.onMessage?.(msg)
    })

    ws.addEventListener('error', (ev) => {
      if (this.closed) return
      this.onError?.(ev)
    })

    ws.addEventListener('close', (ev) => {
      if (this.closed) return
      this.closed = true
      this.onClose?.(ev.code, ev.reason)
    })
  }

  close(code = 1000, reason = 'client closing'): void {
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

function narrowHostSignal(raw: unknown): HostSignal | null {
  if (typeof raw !== 'object' || raw === null) return null
  const v = raw as {
    type?: unknown
    sessionId?: unknown
    reason?: unknown
    sdp?: unknown
    candidate?: unknown
  }
  return match(v.type)
    .with('paired', () =>
      typeof v.sessionId === 'string' ? { type: 'paired' as const, sessionId: v.sessionId } : null,
    )
    .with('rejected', () => ({
      type: 'rejected' as const,
      reason: typeof v.reason === 'string' ? v.reason : 'rejected',
    }))
    .with('accepted', () => ({ type: 'accepted' as const }))
    .with('offer', () =>
      v.sdp ? { type: 'offer' as const, sdp: v.sdp as RTCSessionDescriptionInit } : null,
    )
    .with('answer', () =>
      v.sdp ? { type: 'answer' as const, sdp: v.sdp as RTCSessionDescriptionInit } : null,
    )
    .with('ice', () =>
      v.candidate ? { type: 'ice' as const, candidate: v.candidate as RTCIceCandidateInit } : null,
    )
    .with('bye', () => ({
      type: 'bye' as const,
      reason: typeof v.reason === 'string' ? v.reason : undefined,
    }))
    .otherwise(() => null)
}
