import { methods, type RpcTypes } from './schema'
import { encode, decode, toBase64, fromBase64 } from './protobuf'
import type { NativeRemote, RpcEvent } from './bridge'
import { timing, timingStart } from './metrics'
import type { ChatRef, CommandMeta } from './schema'

type Method = keyof RpcTypes
type Input<M extends Method> = RpcTypes[M]['input']
type Output<M extends Method> = RpcTypes[M]['output']
export class RpcError extends Error {
  readonly code: number
  constructor(code: number, message: string) { super(message); this.code = code }
}
export interface RpcStream<I, O> {
  send(value: I): Promise<void>
  finish(): Promise<void>
  cancel(): void
  done: Promise<void>
}
type Pending = {
  message(bytes: Uint8Array): Promise<void>
  resolve(): void
  reject(error: Error): void
  written?: { resolve(): void; reject(error: Error): void }
  tail: Promise<void>
  removeAbort(): void
}
// Each host owns its native channel, call registry and byte budget; no selected-host global.
export class GrpcTransport {
  private pending = new Map<string, Pending>()
  private sequence = 0
  private generation = 0
  private closed = false
  private incomingBytes = 0
  private outgoingBytes = 0
  private listener: { remove(): void }
  readonly native: NativeRemote
  readonly connectionId: string
  constructor(native: NativeRemote, connectionId: string) {
    this.native = native; this.connectionId = connectionId
    this.listener = native.addListener('rpc', event => this.event(event))
  }
  async open(config: Record<string, string>): Promise<void> {
    if (this.closed) throw Error('transport disposed')
    await this.native.connect(this.connectionId, config)
  }
  async unary<M extends Method>(method: M, request: Input<M>, signal?: AbortSignal, deadlineMs = 15000): Promise<Output<M>> {
    if (methods[method].serverStream || methods[method].clientStream) throw Error('not a unary method')
    let output: Output<M> | undefined
    let count = 0
    const stream = await this.stream(method, value => { output = value; if (++count !== 1) throw Error('multiple unary responses') }, signal, deadlineMs)
    const started = timingStart(), target = request as { command?: CommandMeta; chat?: ChatRef; host_id?: string }
    const host = target.command?.host_id ?? target.chat?.host_id ?? target.host_id
    let ok = false
    try { await stream.send(request); await stream.finish(); await stream.done; ok = output !== undefined }
    catch (error) { stream.cancel(); throw error }
    finally { if (host) timing(host, target.chat?.chat_id ?? host, target.command?.command_id, 'client_round_trip', started, ok) }
    if (output === undefined) throw Error('missing unary response')
    return output
  }
  async subscribe<M extends Method>(method: M, request: Input<M>, onMessage: (value: Output<M>) => void | Promise<void>, signal?: AbortSignal): Promise<RpcStream<Input<M>, Output<M>>> {
    if (!methods[method].serverStream || methods[method].clientStream) throw Error('not a server stream')
    const stream = await this.stream(method, onMessage, signal)
    try { await stream.send(request); await stream.finish(); return stream }
    catch (error) { stream.cancel(); throw error }
  }
  async stream<M extends Method>(method: M, onMessage: (value: Output<M>) => void | Promise<void>, signal?: AbortSignal, deadlineMs = 3600000): Promise<RpcStream<Input<M>, Output<M>>> {
    if (this.closed || signal?.aborted) throw new RpcError(1, 'cancelled')
    if (this.pending.size >= 32) throw new RpcError(8, 'RPC limit reached')
    const descriptor = methods[method]
    const id = `${this.connectionId}:${this.generation}:${++this.sequence}`
    let resolve!: () => void, reject!: (error: Error) => void
    const done = new Promise<void>((ok, fail) => { resolve = ok; reject = fail })
    // Mark handled immediately; callers can still await the original rejection.
    void done.catch(() => {})
    const cancel = () => { this.end(id, new RpcError(1, 'cancelled')); void this.native.cancel(id).catch(() => {}) }
    const state: Pending = {
      message: async bytes => { await onMessage(decode<Output<M>>(descriptor.output, bytes)) },
      resolve, reject, tail: Promise.resolve(), removeAbort: () => signal?.removeEventListener('abort', cancel),
    }
    this.pending.set(id, state)
    signal?.addEventListener('abort', cancel, { once: true })
    try { await this.native.start(this.connectionId, id, descriptor.path, deadlineMs) }
    catch (e) { this.end(id, e as Error); throw e }
    if (signal?.aborted || !this.pending.has(id)) { cancel(); throw new RpcError(1, 'cancelled') }
    return {
      done, cancel,
      send: async value => {
        if (!this.pending.has(id)) throw new RpcError(1, 'call closed')
        if (state.written) throw new RpcError(8, 'previous write is pending')
        const bytes = encode(descriptor.input, value)
        if (this.outgoingBytes + bytes.length > 4 * 1024 * 1024) throw new RpcError(8, 'outbound byte limit')
        this.outgoingBytes += bytes.length
        try {
          await new Promise<void>((ok, fail) => {
            state.written = { resolve: ok, reject: fail }
            this.native.write(id, toBase64(bytes)).catch(error => { state.written = undefined; fail(error) })
          })
        } finally { this.outgoingBytes -= bytes.length }
      },
      finish: () => this.native.finish(id),
    }
  }
  private event(event: RpcEvent): void {
    const state = this.pending.get(event.id)
    if (!state) return
    if (event.kind === 'written') { const written = state.written; state.written = undefined; written?.resolve(); return }
    if (event.kind === 'close') {
      state.tail = state.tail.then(() => this.end(event.id, event.code ? new RpcError(event.code, event.message ?? 'RPC failed') : undefined))
      return
    }
    let bytes: Uint8Array
    try {
      bytes = fromBase64(event.data ?? '')
      if (this.incomingBytes + bytes.length > 4 * 1024 * 1024) throw new RpcError(8, 'resync_required: inbound byte limit')
    } catch (e) { this.end(event.id, e as Error); void this.native.cancel(event.id).catch(() => {}); return }
    this.incomingBytes += bytes.length
    state.tail = state.tail.then(async () => {
      if (this.pending.get(event.id) === state) { await state.message(bytes); await this.native.ack(event.id) }
    }).catch(error => { this.end(event.id, error); void this.native.cancel(event.id).catch(() => {}) })
      .finally(() => { this.incomingBytes -= bytes.length })
  }
  private end(id: string, error?: Error): void {
    const state = this.pending.get(id)
    if (!state) return
    this.pending.delete(id); state.removeAbort()
    state.written?.reject(error ?? new RpcError(1, 'call closed before write acknowledgement'))
    if (error) state.reject(error); else state.resolve()
  }
  async disconnect(): Promise<void> {
    ++this.generation
    for (const id of this.pending.keys()) this.end(id, new RpcError(14, 'disconnected; mutation outcome may be unknown'))
    await this.native.disconnect(this.connectionId)
  }
  async dispose(): Promise<void> {
    this.closed = true
    try { await this.disconnect() } finally { this.listener.remove() }
  }
}
