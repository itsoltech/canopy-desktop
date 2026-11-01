import { recoveryDelay } from './recovery'
import type { HostSession } from './hosts'
import { ChatEventAssembler, ResyncRequired } from './fragments'
import { ChatProjectionOwner, HistoryPageAssembler, type ChatProjection } from './projection'
import type { ThreadRef, ItemKey, TranscriptItem } from './schema'
import { uniqueId } from './commands'
import { timing, timingStart } from './metrics'
export class ChatViewOwner {
  readonly chat: ThreadRef
  private projection = new ChatProjectionOwner()
  private controller?: AbortController
  private listeners = new Set<() => void>()
  private unsubscribe?: () => void
  private hostReady = false
  private generation = 0
  private error?: string
  private historyToken?: string
  private historyLoaded = false
  private historyWindow?: TranscriptItem[]
  private historyAbort?: AbortController
  constructor(readonly host: HostSession, chatId: string) {
    const thread = host.snapshot().catalog?.threads.find(t => t.catalog_id === chatId)?.thread
    if (!thread || thread.host_id !== host.snapshot().hostId) throw Error('Thread identity is unavailable; refresh the host catalog')
    this.chat = thread
  }
  snapshot(): ChatProjection & { error?: string } { return { ...this.projection.snapshot(), items: this.historyWindow ?? this.projection.snapshot().items, error: this.error } }
  subscribe(callback: () => void): () => void { this.listeners.add(callback); return () => { this.listeners.delete(callback) } }
  private publish(): void { for (const listener of this.listeners) listener() }
  start(): void {
    this.unsubscribe?.()
    const changed = () => {
      const ready = this.host.snapshot().phase === 'ready'
      if (ready === this.hostReady) return
      this.hostReady = ready; this.controller?.abort()
      if (ready) void this.connect()
    }
    this.unsubscribe = this.host.subscribe(changed); changed()
  }
  async connect(): Promise<void> {
    const generation = ++this.generation
    this.controller?.abort(); const controller = new AbortController(); this.controller = controller
    for (let attempt = 0; !controller.signal.aborted; attempt++) {
      const assembler = new ChatEventAssembler(this.chat, uniqueId(), this.host.transport.native)
      try {
        this.projection.discardSnapshot()
        const stream = await this.host.transport.subscribe('ThreadService/SubscribeThread', { thread: this.chat }, async event => {
          if (generation !== this.generation || controller.signal.aborted) return
          const received = timingStart()
          let applied = false
          try { await assembler.push(event, value => {
            if (generation !== this.generation || controller.signal.aborted) return
            if (this.projection.apply(value)) { this.error = undefined; this.publish() }
          }); applied = true } finally { timing(this.chat.host_id, this.chat.thread_id, undefined, 'client_receive_apply', received, applied) }
        }, controller.signal)
        await stream.done
        if (!controller.signal.aborted) throw new ResyncRequired('Chat subscription ended')
      } catch (error) {
        if (!controller.signal.aborted) { this.error = error instanceof Error ? error.message : String(error); this.publish() }
      } finally { await assembler.reset(); if (generation === this.generation) this.projection.discardSnapshot() }
      await recoveryDelay(attempt, controller.signal)
    }
  }
  async earlier(): Promise<void> {
    if (this.historyLoaded && !this.historyToken) return
    this.historyAbort?.abort(); const abort = new AbortController(); this.historyAbort = abort
    const page = new HistoryPageAssembler(this.chat, uniqueId(), this.host.transport.native)
    try {
      const stream = await this.host.transport.subscribe('ThreadService/ReadHistoryPage', { thread: this.chat, limit: 100, page_token: this.historyToken, before: this.historyToken ? undefined : this.snapshot().items[0]?.key }, value => page.push(value), abort.signal)
      await stream.done
      if (abort.signal.aborted) return
      const result = page.result()
      if (this.historyWindow) {
        const keys = new Set(this.historyWindow.map(item => `${item.key?.history_epoch}:${item.key?.item_id}`))
        this.historyWindow = [...result.items.filter(item => !keys.has(`${item.key?.history_epoch}:${item.key?.item_id}`)), ...this.historyWindow]
      } else this.projection.prepend(result.items)
      this.historyToken = result.previous; this.historyLoaded = true
      this.error = result.incomplete ? 'Some history could not be read' : undefined; this.publish()
    } finally { await page.dispose() }
  }
  async jump(around: ItemKey): Promise<void> {
    this.historyAbort?.abort(); const abort = new AbortController(); this.historyAbort = abort
    const page = new HistoryPageAssembler(this.chat, uniqueId(), this.host.transport.native)
    try {
      const stream = await this.host.transport.subscribe('ThreadService/ReadHistoryPage', { thread: this.chat, limit: 100, around }, value => page.push(value), abort.signal)
      await stream.done
      if (abort.signal.aborted) return
      const result = page.result()
      if (!result.items.some(item => item.key?.history_epoch === around.history_epoch && item.key.item_id === around.item_id)) throw Error('History anchor is no longer available')
      this.historyWindow = result.items; this.historyToken = result.previous; this.historyLoaded = true; this.error = result.incomplete ? 'Some history could not be read' : undefined; this.publish()
    } finally { await page.dispose() }
  }
  live(): void { this.historyAbort?.abort(); this.historyWindow = undefined; this.historyToken = undefined; this.historyLoaded = false; this.publish() }
  dispose(): void { this.hostReady = false; ++this.generation; this.historyAbort?.abort(); this.unsubscribe?.(); this.controller?.abort(); this.listeners.clear() }
}
