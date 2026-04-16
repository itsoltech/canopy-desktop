// Keep in sync with src/renderer-shared/rpc/DataChannelRpc.ts when the wire protocol changes.
//
// Runtime note: `crypto.getRandomValues()` requires the
// `react-native-get-random-values` polyfill to be imported at app entry
// before this module loads.

import type { RpcMessage } from './protocol/rpc-types'

/**
 * Generate a short random identifier for correlating RPC requests with
 * their responses. We deliberately avoid `crypto.randomUUID()` because that
 * API is only defined on *secure contexts* (HTTPS / localhost).
 *
 * `crypto.getRandomValues()` is available in both secure and insecure
 * contexts (and in React Native once `react-native-get-random-values` is
 * installed), so we use it to build a hex id ourselves.
 */
function randomRpcId(): string {
  const bytes = new Uint8Array(8)
  crypto.getRandomValues(bytes)
  let hex = ''
  for (let i = 0; i < bytes.length; i++) {
    hex += bytes[i].toString(16).padStart(2, '0')
  }
  return hex
}

/**
 * Symmetric JSON-RPC engine layered over a single {@link RTCDataChannel}.
 *
 * The "symmetric" part is important: both peers run a `DataChannelRpc`
 * against the same channel. Either side can:
 *
 *   - **call** remote methods registered on the peer
 *   - **register** handlers the peer may call
 *   - **subscribe** to topics the peer emits
 *   - **emit** topic events the peer may subscribe to
 *
 * Every frame is a single JSON line; the underlying SCTP transport
 * preserves message boundaries so we don't need length prefixes.
 */

type PendingRequest = {
  resolve: (v: unknown) => void
  reject: (e: Error) => void
  timer: ReturnType<typeof setTimeout>
}

type MethodHandler = (params: unknown) => Promise<unknown> | unknown

type EventHandler = (data: unknown) => void

export interface DataChannelRpcOptions {
  /** How long to wait for a response before rejecting a `call`. Default 30s. */
  timeoutMs?: number
  /** Tag for console logs so the two peers can be told apart. */
  label?: string
}

export class DataChannelRpc {
  private pending = new Map<string, PendingRequest>()
  private topicSubscribers = new Map<string, Set<EventHandler>>()
  private methodHandlers = new Map<string, MethodHandler>()
  private timeoutMs: number
  private label: string
  private disposed = false

  constructor(
    private channel: RTCDataChannel,
    opts: DataChannelRpcOptions = {},
  ) {
    this.timeoutMs = opts.timeoutMs ?? 30_000
    this.label = opts.label ?? 'rpc'
    channel.addEventListener('message', this.handleMessage as EventListener)
    channel.addEventListener('close', this.handleClose as EventListener)
  }

  /**
   * Make an RPC call to the peer. Resolves with the peer's `result` on success
   * or rejects with an Error whose message is the tagged error message from
   * the peer. Times out after `timeoutMs` with a generic `RPC_TIMEOUT` error.
   */
  call<R = unknown>(method: string, params?: unknown): Promise<R> {
    if (this.disposed) return Promise.reject(new Error('RPC_DISPOSED'))
    return new Promise<R>((resolve, reject) => {
      const id = randomRpcId()
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`RPC_TIMEOUT: ${method}`))
      }, this.timeoutMs)
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject, timer })
      const ok = this.sendFrame({ kind: 'request', id, method, params })
      if (!ok) {
        clearTimeout(timer)
        this.pending.delete(id)
        reject(new Error('RPC_CHANNEL_CLOSED'))
      }
    })
  }

  /**
   * Register a handler for an inbound `method`. Replaces any prior handler
   * for the same name silently.
   */
  registerMethod(method: string, handler: MethodHandler): void {
    this.methodHandlers.set(method, handler)
  }

  /**
   * Subscribe to topic events emitted by the peer. Returns an unsubscribe
   * function.
   */
  subscribe(topic: string, handler: EventHandler): () => void {
    let set = this.topicSubscribers.get(topic)
    if (!set) {
      set = new Set()
      this.topicSubscribers.set(topic, set)
    }
    set.add(handler)
    return () => {
      set!.delete(handler)
      if (set!.size === 0) this.topicSubscribers.delete(topic)
    }
  }

  /**
   * Emit a topic event to the peer. Fire-and-forget.
   */
  emit(topic: string, data: unknown): void {
    if (this.disposed) return
    this.sendFrame({ kind: 'event', topic, data })
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.channel.removeEventListener('message', this.handleMessage as EventListener)
    this.channel.removeEventListener('close', this.handleClose as EventListener)
    for (const { reject, timer } of this.pending.values()) {
      clearTimeout(timer)
      reject(new Error('RPC_DISPOSED'))
    }
    this.pending.clear()
    this.topicSubscribers.clear()
    this.methodHandlers.clear()
  }

  // ===== message handling =====

  private handleMessage = (ev: MessageEvent): void => {
    let text: string
    if (typeof ev.data === 'string') {
      text = ev.data
    } else {
      console.warn(`[${this.label}] dropped non-text frame`)
      return
    }
    let parsed: RpcMessage
    try {
      parsed = JSON.parse(text) as RpcMessage
    } catch {
      console.warn(`[${this.label}] bad JSON frame`)
      return
    }
    switch (parsed.kind) {
      case 'request':
        void this.handleRequest(parsed.id, parsed.method, parsed.params)
        return
      case 'response':
        this.handleResponse(parsed)
        return
      case 'event':
        this.handleEvent(parsed.topic, parsed.data)
        return
      default:
        console.warn(`[${this.label}] unknown frame kind`, parsed)
    }
  }

  private handleClose = (): void => {
    if (this.disposed) return
    for (const { reject, timer } of this.pending.values()) {
      clearTimeout(timer)
      reject(new Error('RPC_CHANNEL_CLOSED'))
    }
    this.pending.clear()
  }

  private async handleRequest(id: string, method: string, params: unknown): Promise<void> {
    const handler = this.methodHandlers.get(method)
    if (!handler) {
      this.sendFrame({
        kind: 'response',
        id,
        ok: false,
        error: { _tag: 'METHOD_NOT_FOUND', message: `Unknown method: ${method}` },
      })
      return
    }
    try {
      const result = await handler(params)
      this.sendFrame({ kind: 'response', id, ok: true, result })
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      this.sendFrame({
        kind: 'response',
        id,
        ok: false,
        error: { _tag: 'HANDLER_ERROR', message },
      })
    }
  }

  private handleResponse(frame: Extract<RpcMessage, { kind: 'response' }>): void {
    const pending = this.pending.get(frame.id)
    if (!pending) return
    clearTimeout(pending.timer)
    this.pending.delete(frame.id)
    if (frame.ok) {
      pending.resolve(frame.result)
    } else {
      pending.reject(new Error(`${frame.error._tag}: ${frame.error.message}`))
    }
  }

  private handleEvent(topic: string, data: unknown): void {
    const set = this.topicSubscribers.get(topic)
    if (!set) return
    for (const handler of set) {
      try {
        handler(data)
      } catch (err) {
        console.warn(`[${this.label}] subscriber threw for topic "${topic}":`, err)
      }
    }
  }

  /**
   * Write a frame to the channel. Returns `false` if the channel isn't open
   * so callers can fail fast without waiting for the timeout.
   */
  private sendFrame(msg: RpcMessage): boolean {
    if (this.channel.readyState !== 'open') return false
    try {
      this.channel.send(JSON.stringify(msg))
      return true
    } catch (err) {
      console.warn(`[${this.label}] send failed:`, err)
      return false
    }
  }
}
