import { uniqueId } from './commands'
import { readCatalog, type CatalogStreamClient } from './catalog'
import { HostStore, type OutboxEntry, type LocalImage } from './persistence'
import { type CommandReceipt, type MessageDraft, type SendContext, type ThreadStatus, type HostInfo, type RpcTypes } from './schema'

type Method = keyof RpcTypes
export interface UnaryClient extends CatalogStreamClient {
  unary<M extends Method>(method: M, request: RpcTypes[M]['input'], signal?: AbortSignal): Promise<RpcTypes[M]['output']>
}
function sameContext(expected: SendContext, actual: ThreadStatus): boolean {
  return expected.account_id === actual.agent_id && expected.profile_id === actual.profile_id && expected.history_epoch === actual.history_epoch
}
const final = new Set<number>([3, 4, 5])
// Single owner per host/device serializes epoch changes and sends across all views.
export class ClientOutbox {
  private gate: Promise<unknown> = Promise.resolve()
  readonly store: HostStore
  constructor(store: HostStore, private readonly prepareImages?: (entry: OutboxEntry) => Promise<OutboxEntry>) { this.store = store }
  private exclusive<T>(work: () => Promise<T>): Promise<T> {
    const result = this.gate.then(work); this.gate = result.catch(() => {}); return result
  }
  async saveDraft(chatId: string, context: SendContext, draft: MessageDraft, images?: LocalImage[], revision = uniqueId()): Promise<void> {
    await this.store.update(state => { state.drafts[chatId] = { revision, context, draft, images }; return null })
  }
  async clearDraft(chatId: string, revision: string): Promise<boolean> {
    return this.store.update(state => {
      if (state.drafts[chatId]?.revision !== revision) return false
      delete state.drafts[chatId]
      return true
    })
  }
  async enqueue(entry: OutboxEntry, revision?: string): Promise<boolean> {
    if (entry.draft.attachments.some(attachment => !attachment.image)) throw Error('Contextual attachments require an online send; the draft is retained')
    return this.store.update(state => {
      const c = state.connection
      if (entry.hostId !== c.hostId || entry.deviceId !== c.deviceId || entry.epoch !== undefined || entry.receipt) throw Error('Invalid unsent outbox identity')
      if (state.outbox.some(e => e.commandId === entry.commandId || e.messageId === entry.messageId)) throw Error('Duplicate outbox identity')
      if (!entry.commandId || !entry.messageId || !entry.chatId) throw Error('Missing outbox identity')
      state.outbox.push(entry)
      const cleared = revision !== undefined && state.drafts[entry.chatId]?.revision === revision
      if (cleared) delete state.drafts[entry.chatId]
      return cleared
    })
  }
  sendExplicit(commandId: string, client: UnaryClient, confirmedContext?: SendContext): Promise<CommandReceipt> {
    return this.exclusive(async () => {
      let state = this.store.snapshot()
      let entry = state.outbox.find(e => e.commandId === commandId)
      if (!entry) throw Error('Outbox entry missing')
      if (entry.epoch === undefined && entry.draft.attachments.some(attachment => !attachment.image)) throw Error('Contextual attachments require an online send; restore this message as a draft')
      if (entry.epoch !== undefined) return this.reconcileEntry(entry, client, true)
      if (state.closingEpoch !== undefined) throw Error('Command epoch closure is unresolved; text retained')
      const host = state.connection.hostId
      const [info, catalog] = await Promise.all([
        client.unary('HostService/GetInfo', { environment: { host_id: host } }),
        readCatalog(client, host),
      ])
      this.validateInfo(info)
      if (catalog.environment?.host_id !== host) throw Error('Host identity mismatch')
      const listed = catalog.threads.find(c => c.catalog_id === entry!.chatId && c.thread?.host_id === host)
      const chat = listed?.summary
      if (!chat || !listed?.thread) throw Error('Chat was removed; outbox text retained')
      // Account cannot be silently changed. Explicit context confirmation is only for a branch change.
      if (chat.agent_id !== entry.context.account_id || chat.profile_id !== entry.context.profile_id) throw Error('Chat account changed; outbox text retained')
      if (!sameContext(entry.context, chat)) {
        if (!confirmedContext || !sameContext(confirmedContext, chat)) throw Error('Chat context changed; explicit confirmation required')
        entry = { ...entry, context: confirmedContext }
      }
      if (entry.images?.length) {
        if (entry.images.length > 10 || !this.prepareImages) throw Error('Image preparation unavailable')
        entry = await this.prepareImages(entry)
      }
      const identity = await client.unary('CatalogService/ListThreads', { environment: { host_id: host }, catalog_id: entry.chatId, limit: 1 })
      const target = identity.threads[0]?.target
      if (identity.threads.length !== 1 || target?.thread?.host_id !== host || target.thread.thread_id !== listed.thread.thread_id) throw Error('Thread identity changed; outbox text retained')
      const prepared = { ...entry, target, epoch: info.command_epoch }
      // Persist the exact immutable envelope before any RPC can escape.
      await this.store.update(next => {
        if (next.closingEpoch !== undefined) throw Error('Command epoch closing')
        next.epoch = info.command_epoch
        next.outbox = next.outbox.map(e => e.commandId === commandId ? prepared : e)
        return null
      })
      return this.transmit(prepared, client)
    })
  }
  reconcile(client: UnaryClient): Promise<void> {
    return this.exclusive(async () => {
      // Reconnect only queries previous attempts. It never sends unsent text or replays effects.
      for (const entry of this.store.snapshot().outbox) {
        if (entry.epoch !== undefined && !entry.receipt?.resolved_by_user && !final.has(entry.receipt?.state ?? 0)) await this.reconcileEntry(entry, client, false)
      }
    })
  }
  private async reconcileEntry(entry: OutboxEntry, client: UnaryClient, retryExplicit: boolean): Promise<CommandReceipt> {
    if (entry.receipt && (final.has(entry.receipt.state) || entry.receipt.resolved_by_user)) return entry.receipt
    if (!entry.target?.thread) throw Error('Saved thread identity is unavailable; original text retained')
    const result = await client.unary('HostService/GetCommand', { environment: { host_id: entry.hostId }, command_id: entry.commandId, command_epoch: entry.epoch!, resource: { thread: entry.target } })
    if (result.state === 2 && retryExplicit) { if (entry.epoch !== this.store.snapshot().epoch) throw Error('Saved command epoch was retired; the request cannot be replayed'); return this.transmit(entry, client) }
    const receipt = result.receipt
    if (!receipt) throw Error(result.state === 3 ? 'Command epoch retired; original text retained' : 'Command result not found; original text retained')
    this.validateReceipt(entry, receipt)
    await this.record(entry, receipt)
    return receipt
  }
  private async transmit(entry: OutboxEntry, client: UnaryClient): Promise<CommandReceipt> {
    if (!entry.target?.thread || entry.target.thread.host_id !== entry.hostId) throw Error('Saved thread identity is unavailable; original text retained')
    const receipt = await client.unary('ThreadService/SendMessage', {
      command: { command_id: entry.commandId, command_epoch: entry.epoch! },
      target: entry.target, message_id: entry.messageId,
      draft: entry.draft, expected_context: entry.context,
    })
    this.validateReceipt(entry, receipt); await this.record(entry, receipt); return receipt
  }
  private validateInfo(info: HostInfo): void {
    if (info.environment?.host_id !== this.store.snapshot().connection.hostId || info.protocol_version !== 1) throw Error('Incompatible host identity or protocol')
  }
  private validateReceipt(entry: OutboxEntry, receipt: CommandReceipt): void {
    if (receipt.host_id !== entry.hostId || receipt.command_id !== entry.commandId || receipt.command_epoch !== entry.epoch) throw Error('Command receipt identity mismatch')
  }
  private async record(entry: OutboxEntry, receipt: CommandReceipt): Promise<void> {
    await this.store.update(state => { const target = state.outbox.find(e => e.commandId === entry.commandId); if (target) target.receipt = receipt; return null })
  }
  dismiss(commandId: string): Promise<void> {
    return this.exclusive(async () => {
      await this.store.update(state => {
        const entry = state.outbox.find(e => e.commandId === commandId)
        if (entry?.epoch !== undefined && (!entry.receipt || (!final.has(entry.receipt.state) && !entry.receipt.resolved_by_user))) throw Error('Unresolved command must be reconciled before removal')
        state.outbox = state.outbox.filter(e => e.commandId !== commandId); return null
      })
    })
  }
  closeEpoch(client: UnaryClient, retireUnresolved = false): Promise<void> {
    return this.exclusive(async () => {
      let state = this.store.snapshot()
      if (state.epoch === undefined) throw Error('Command epoch not synchronized')
      if (!retireUnresolved && state.outbox.some(e => e.epoch !== undefined && (!e.receipt || (!final.has(e.receipt.state) && !e.receipt.resolved_by_user)))) throw Error('Unresolved commands block epoch closure')
      const expected = state.closingEpoch ?? state.epoch
      await this.store.update(next => {
        // Remove confirmed retry records before closing. Retain unknown and all unsent text.
        next.outbox = next.outbox.filter(e => !e.receipt || !final.has(e.receipt.state))
        next.closingEpoch = expected; return null
      })
      if (retireUnresolved) {
        const records = [
          ...state.outbox.filter(e => e.epoch !== undefined).map(e => ({ commandId: e.commandId, epoch: e.epoch!, receipt: e.receipt })),
          ...(state.mutations ?? []),
        ]
        for (const record of records) {
          if (record.receipt?.resolved_by_user || final.has(record.receipt?.state ?? 0)) continue
          const request = { environment: { host_id: state.connection.hostId }, command_id: record.commandId, command_epoch: record.epoch }
          const result = await client.unary('HostService/GetCommand', request)
          let receipt = result.receipt
          if (!receipt) throw Error('Command result unavailable; original intent retained')
          const validate = () => { if (!receipt || receipt.host_id !== state.connection.hostId || receipt.command_id !== request.command_id || receipt.command_epoch !== request.command_epoch) throw Error('Reviewed command identity mismatch') }
          validate()
          if (receipt.state === 6 && !receipt.resolved_by_user) {
            receipt = (await client.unary('HostService/ResolveCommand', request)).receipt
            validate()
            if (!receipt?.resolved_by_user) throw Error('Host did not acknowledge operation review')
          }
          await this.store.update(next => {
            const outbox = next.outbox.find(e => e.commandId === record.commandId && e.epoch === record.epoch)
            const mutation = next.mutations?.find(e => e.commandId === record.commandId && e.epoch === record.epoch)
            if (outbox) outbox.receipt = receipt
            if (mutation) mutation.receipt = receipt
            return null
          })
        }
      }
      const epoch = await client.unary('HostService/CloseCommandEpoch', { environment: { host_id: state.connection.hostId }, expected_epoch: expected })
      if (epoch.environment?.host_id !== state.connection.hostId || BigInt(epoch.epoch) !== BigInt(expected) + 1n) throw Error('Unexpected epoch closure result')
      await this.store.update(next => { next.epoch = epoch.epoch; delete next.closingEpoch; return null })
    })
  }
}

// One synchronous owner for every composer edit, including attachment-only changes.
// Storage completion must never install an older edit back into the visible composer.
export type ComposerSnapshot = { revision: string; context?: SendContext; draft: MessageDraft; images: LocalImage[] }
export class ComposerDraft {
  private value: ComposerSnapshot
  constructor(private outbox: ClientOutbox, private chatId: string) {
    const stored = outbox.store.snapshot().drafts[chatId]
    this.value = { revision: stored?.revision ?? uniqueId(), context: stored?.context,
      draft: stored?.draft ?? { text: '', attachments: [], goal: false }, images: stored?.images ?? [] }
  }
  snapshot(): ComposerSnapshot { return this.value }
  edit(patch: { text?: string; attachments?: MessageDraft['attachments']; images?: LocalImage[] }, context?: SendContext): Promise<void> {
    const old = this.value
    this.value = { revision: uniqueId(), context: context ?? old.context, draft: {
      ...old.draft, text: patch.text ?? old.draft.text, attachments: patch.attachments ?? old.draft.attachments,
    }, images: patch.images ?? old.images }
    return this.persist()
  }
  persist(): Promise<void> {
    const value = this.value
    if (!value.context) return Promise.reject(Error('Load the original chat context before composing'))
    return this.outbox.saveDraft(this.chatId, value.context, value.draft, value.images, value.revision)
  }
  async clear(submitted: ComposerSnapshot, alreadyCleared = false): Promise<boolean> {
    const cleared = alreadyCleared || await this.outbox.clearDraft(this.chatId, submitted.revision)
    if (!cleared || this.value.revision !== submitted.revision) return false
    this.value = { revision: uniqueId(), draft: { text: '', attachments: [], goal: false }, images: [] }
    return true
  }
}
