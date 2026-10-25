import { ChatEventAssembler, ResyncRequired } from './fragments'
import type { NativeRemote } from './bridge'
import type { ChatEvent, ChatRef, ChatSummary, ChatQueue, TranscriptItem, HistoryPageStart, HistoryPageEvent } from './schema'
import { encode } from './protobuf'

const key = (item: TranscriptItem) => {
  if (!item.key) throw new ResyncRequired('Missing item identity')
  return `${item.key.history_epoch}:${item.key.item_id}`
}
export type ChatProjection = { items: TranscriptItem[]; summary?: ChatSummary; queue?: ChatQueue; savedSequence: string; hasEarlier: boolean }
const empty = (): ChatProjection => ({ items: [], savedSequence: '0', hasEarlier: false })
/** A view publishes only complete snapshots and verified item revisions. */
export class ChatProjectionOwner {
  private value = empty()
  private staging?: ChatProjection
  private bytes = 0
  snapshot(): ChatProjection { return this.value }
  discardSnapshot(): void { this.staging = undefined; this.bytes = 0 }
  apply(event: ChatEvent): boolean {
    if (event.snapshot_start) {
      if (this.staging) throw new ResyncRequired('Nested snapshot')
      this.staging = { ...empty(), summary: event.snapshot_start.summary, queue: event.snapshot_start.queue, savedSequence: event.snapshot_start.saved_sequence }
      this.bytes = 0; return false
    }
    if (event.snapshot_end) {
      if (!this.staging) throw new ResyncRequired('Snapshot end without start')
      this.value = { ...this.staging, hasEarlier: event.snapshot_end.has_earlier_history }; this.discardSnapshot(); return true
    }
    const current = this.staging ?? this.value
    let next = { ...current }
    if (event.snapshot_items) {
      if (!this.staging) throw new ResyncRequired('Snapshot items outside snapshot')
      for (const item of event.snapshot_items.items) {
        this.bytes += encode('TranscriptItem', item, 0, 32 * 1024 * 1024).length
        if (this.bytes > 64 * 1024 * 1024 || next.items.length >= 2000) throw new ResyncRequired('Snapshot projection budget exceeded')
        if (next.items.some(i => key(i) === key(item))) throw new ResyncRequired('Duplicate snapshot item')
        next.items = [...next.items, item]
      }
    } else if (event.replace_item) {
      const item = event.replace_item, existing = current.items.find(i => key(i) === key(item))
      if (existing && BigInt(item.revision) <= BigInt(existing.revision)) throw new ResyncRequired('Invalid item revision')
      next.items = existing ? current.items.map(i => key(i) === key(item) ? item : i) : [...current.items, item]
    } else if (event.append_text) {
      const patch = event.append_text
      const index = current.items.findIndex(i => i.key?.history_epoch === patch.key?.history_epoch && i.key?.item_id === patch.key?.item_id)
      const item = current.items[index], body = item?.text ?? item?.thinking
      if (!item || !body || item.revision !== patch.expected_revision || BigInt(patch.revision) <= BigInt(item.revision) || BigInt(new TextEncoder().encode(body.text).length) !== BigInt(patch.utf8_offset)) throw new ResyncRequired('Text revision or offset conflict')
      const changed = { ...item, revision: patch.revision, [item.text ? 'text' : 'thinking']: { text: body.text + patch.text, streaming: patch.streaming } }
      next.items = current.items.map((item, i) => i === index ? changed : item)
    } else if (event.queue_changed) next.queue = event.queue_changed
    else if (event.status) next.summary = event.status
    else if (event.saved_sequence !== undefined) next.savedSequence = event.saved_sequence
    else if (event.decision) next.items = current.items.map(item => item.decision?.decision_id === event.decision!.decision_id ? { ...item, decision: event.decision } : item)
    else throw new ResyncRequired('Unexpected chat event')
    if (this.staging) { this.staging = next; return false }
    this.value = next; return true
  }
  prepend(items: TranscriptItem[]): void {
    const existing = new Set(this.value.items.map(key))
    this.value = { ...this.value, items: [...items.filter(i => !existing.has(key(i))), ...this.value.items] }
  }
}
/** Uses the same verified fragment assembler without assigning a live cursor to page data. */
export class HistoryPageAssembler {
  private readonly assembler: ChatEventAssembler
  private readonly owner = new ChatProjectionOwner()
  private start?: HistoryPageStart
  private complete = false
  private readonly boundary = { runtime_epoch: 'history', stream_id: 'page', sequence: '0' }
  constructor(private readonly chat: ChatRef, id: string, native: NativeRemote) { this.assembler = new ChatEventAssembler(chat, id, native) }
  async push(event: HistoryPageEvent): Promise<void> {
    if (this.complete || event.chat?.host_id !== this.chat.host_id || event.chat.chat_id !== this.chat.chat_id) throw new ResyncRequired('History identity mismatch')
    let frame: ChatEvent
    if (event.start) {
      if (this.start || !event.start.read_id) throw new ResyncRequired('History start conflict')
      this.start = event.start
      frame = { chat: this.chat, snapshot_start: { boundary: this.boundary, saved_sequence: '0' } }
    } else {
      if (!this.start) throw new ResyncRequired('Missing history start')
      if (event.items) frame = { chat: this.chat, snapshot_items: event.items }
      else if (event.fragment) frame = { chat: this.chat, item_fragment: event.fragment }
      else if (event.end_read_id === this.start.read_id) frame = { chat: this.chat, snapshot_end: { boundary: this.boundary, has_earlier_history: !!this.start.previous_page_token } }
      else throw new ResyncRequired('History boundary mismatch')
    }
    await this.assembler.push(frame, value => { this.owner.apply(value) })
    if (event.end_read_id) this.complete = true
  }
  result(): { items: TranscriptItem[]; previous?: string; incomplete: boolean } {
    if (!this.complete) throw new ResyncRequired('Incomplete history page')
    return { items: this.owner.snapshot().items, previous: this.start!.previous_page_token, incomplete: this.start!.incomplete }
  }
  async dispose(): Promise<void> { await this.assembler.reset(); this.owner.discardSnapshot() }
}
