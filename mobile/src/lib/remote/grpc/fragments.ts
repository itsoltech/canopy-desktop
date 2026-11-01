import type { NativeRemote } from './bridge'
import { decode, toBase64 } from './protobuf'
import type { ThreadEvent, ThreadRef, Cursor, ItemFragment, TranscriptItem } from './schema'

const MAX_ITEM = 32 * 1024 * 1024
const sameCursor = (a: Cursor, b: Cursor) => a.runtime_epoch === b.runtime_epoch && a.stream_id === b.stream_id && a.sequence === b.sequence
export class ResyncRequired extends Error {}
/** One active item assembly per subscription. Transport ACK waits for push(). */
export class ChatEventAssembler {
  private partial?: { fragment: ItemFragment; bytes: Uint8Array; offset: number; hashId: string }
  private received?: Cursor
  private committed?: Cursor
  private snapshotBoundary?: Cursor
  private sequence = 0
  readonly chat: ThreadRef
  private readonly hash: Pick<NativeRemote, 'startHash' | 'hashChunk' | 'finishHash' | 'cancelHash'>
  private readonly subscriptionId: string
  constructor(chat: ThreadRef, subscriptionId: string, hash: Pick<NativeRemote, 'startHash' | 'hashChunk' | 'finishHash' | 'cancelHash'>, since?: Cursor) {
    this.chat = chat; this.subscriptionId = subscriptionId; this.hash = hash; this.received = since; this.committed = since
  }
  resumeCursor(): Cursor | undefined { return this.committed ? { ...this.committed } : undefined }
  async push(event: ThreadEvent, install: (event: ThreadEvent) => void | Promise<void>): Promise<void> {
    try { await this.apply(event, install) }
    catch (error) { await this.reset(); throw error }
  }
  private async apply(event: ThreadEvent, install: (event: ThreadEvent) => void | Promise<void>): Promise<void> {
    if (event.thread?.host_id !== this.chat.host_id || event.thread.thread_id !== this.chat.thread_id) throw new ResyncRequired('Thread identity mismatch')
    if (event.resync) throw new ResyncRequired(event.resync.reason)
    if (event.snapshot_start) {
      if (this.partial || this.snapshotBoundary || !event.snapshot_start.boundary) throw new ResyncRequired('Invalid snapshot start')
      this.snapshotBoundary = event.snapshot_start.boundary
      await install(event); return
    }
    if (this.snapshotBoundary) {
      if (event.cursor) throw new ResyncRequired('Live event during snapshot')
      if (event.snapshot_end) {
        if (this.partial || !event.snapshot_end.boundary || !sameCursor(this.snapshotBoundary, event.snapshot_end.boundary)) throw new ResyncRequired('Incomplete snapshot')
        await install(event)
        this.committed = this.received = this.snapshotBoundary; this.snapshotBoundary = undefined; return
      }
    } else {
      if (!event.cursor || !this.received) throw new ResyncRequired('Missing stream cursor')
      if (event.cursor.runtime_epoch !== this.received.runtime_epoch || event.cursor.stream_id !== this.received.stream_id) throw new ResyncRequired('Stream epoch changed')
      if (BigInt(event.cursor.sequence) <= BigInt(this.received.sequence)) return
      if (BigInt(event.cursor.sequence) !== BigInt(this.received.sequence) + 1n) throw new ResyncRequired('Stream sequence gap')
      this.received = event.cursor
    }
    if (event.item_fragment) {
      const f = event.item_fragment, total = Number(f.total_bytes), offset = Number(f.offset)
      if (!f.key || !f.transfer_id || f.sha256.length !== 32 || !Number.isSafeInteger(total) || total <= 0 || total > MAX_ITEM || f.data.length > 65536 || !f.data.length) throw new ResyncRequired('Invalid item fragment')
      if (!this.partial) {
        if (offset !== 0) throw new ResyncRequired('Missing first item fragment')
        const hashId = `${this.subscriptionId}:${++this.sequence}`
        await this.hash.startHash(hashId)
        this.partial = { fragment: f, bytes: new Uint8Array(total), offset: 0, hashId }
      }
      const p = this.partial, first = p.fragment
      if (f.transfer_id !== first.transfer_id || f.revision !== first.revision || f.key.history_epoch !== first.key!.history_epoch || f.key.item_id !== first.key!.item_id || f.total_bytes !== first.total_bytes || toBase64(f.sha256) !== toBase64(first.sha256) || offset !== p.offset || offset + f.data.length > total) throw new ResyncRequired('Conflicting item fragment')
      p.bytes.set(f.data, offset); p.offset += f.data.length
      await this.hash.hashChunk(p.hashId, toBase64(f.data))
      if (p.offset !== total) return
      if (await this.hash.finishHash(p.hashId) !== toBase64(first.sha256)) throw new ResyncRequired('Item digest mismatch')
      const item = decode<TranscriptItem>('TranscriptItem', p.bytes, 0, MAX_ITEM)
      if (item.key?.history_epoch !== f.key.history_epoch || item.key.item_id !== f.key.item_id || item.revision !== f.revision) throw new ResyncRequired('Assembled item identity mismatch')
      // The owner installs the complete decoded item atomically. No partial text is exposed.
      const replacement: ThreadEvent = this.snapshotBoundary
        ? { thread: this.chat, snapshot_items: { items: [item] } }
        : { thread: this.chat, cursor: event.cursor, replace_item: item }
      await install(replacement)
      this.partial = undefined
    } else {
      if (this.partial) throw new ResyncRequired('Interleaved item fragments')
      await install(event)
    }
    if (!this.snapshotBoundary) this.committed = this.received
  }
  async reset(): Promise<void> {
    const p = this.partial; this.partial = undefined
    this.snapshotBoundary = undefined; this.received = this.committed
    if (p) await this.hash.cancelHash(p.hashId)
  }
}

/** Bounded frame assembly; the caller owns stream sequence and atomic installation. */
export class TerminalFrameAssembler {
  private partial?: { id: string; total: number; digest: string; bytes: Uint8Array; offset: number; hashId: string }
  private sequence = 0
  constructor(private readonly subscriptionId: string, private readonly hash: Pick<NativeRemote, 'startHash' | 'hashChunk' | 'finishHash' | 'cancelHash'>) {}
  get pending() { return !!this.partial }
  async push(f: import('./schema').TerminalFrameFragment): Promise<import('./schema').TerminalFrame | undefined> {
    try {
      const total = Number(f.total_bytes), offset = Number(f.offset), digest = toBase64(f.sha256)
      if (!f.frame_id || f.sha256.length !== 32 || !Number.isSafeInteger(total) || total <= 0 || total > MAX_ITEM || !Number.isSafeInteger(offset) || !f.data.length || f.data.length > 65536) throw new ResyncRequired('Invalid terminal fragment')
      if (!this.partial) {
        if (offset !== 0) throw new ResyncRequired('Missing first terminal fragment')
        const hashId = this.subscriptionId + ':' + ++this.sequence
        await this.hash.startHash(hashId)
        this.partial = { id: f.frame_id, total, digest, bytes: new Uint8Array(total), offset: 0, hashId }
      }
      const p = this.partial
      if (p.id !== f.frame_id || p.total !== total || p.digest !== digest || offset !== p.offset || offset + f.data.length > total) throw new ResyncRequired('Conflicting terminal fragment')
      p.bytes.set(f.data, offset); p.offset += f.data.length
      await this.hash.hashChunk(p.hashId, toBase64(f.data))
      if (p.offset !== total) return
      if (await this.hash.finishHash(p.hashId) !== digest) throw new ResyncRequired('Terminal digest mismatch')
      const frame = decode<import('./schema').TerminalFrame>('TerminalFrame', p.bytes, 0, MAX_ITEM)
      this.partial = undefined
      return frame
    } catch (error) { await this.reset(); throw error }
  }
  async reset() { const p = this.partial; this.partial = undefined; if (p) await this.hash.cancelHash(p.hashId) }
}
