import { chatRequest, retryChatMutation } from './chat-commands'
import { nativeRemote, type NativeRemote } from './native'
import { GrpcTransport, RpcError } from './transport'
import { HostRegistry, HostStore, type HostConnection } from './persistence'
import { prepareUpload, upload } from './uploads'
import { fromBase64 } from './protobuf'
import { ClientOutbox } from './outbox'
import { CatalogFeed } from './catalog'
import type { Catalog, HostInfo, HostStatus, RpcTypes, CommandMeta, CommandReceipt } from './schema'

export type HostSessionState = {
  hostId: string
  phase: 'offline' | 'connecting' | 'ready' | 'suspended' | 'error'
  health?: number
  stale: boolean
  lastSeen?: string
  info?: HostInfo
  status?: HostStatus
  catalog?: Catalog
  error?: string
}
export class HostSession {
  readonly store: HostStore
  readonly outbox: ClientOutbox
  readonly transport: GrpcTransport
  private generation = 0
  private opening?: Promise<void>
  private controller?: AbortController
  private reconnectTimer?: ReturnType<typeof setTimeout>
  private attempts = 0
  private foreground = true
  private disposed = false
  private listeners = new Set<() => void>()
  private state: HostSessionState
  constructor(native: NativeRemote, store: HostStore) {
    this.store = store; this.outbox = new ClientOutbox(store, async entry => {
      const attachments = entry.draft.attachments.filter(a => !a.image)
      for (const image of entry.images ?? []) {
        let uploadId = image.uploadId
        if (!uploadId) {
          const journal = await prepareUpload(this, { chat: { host_id: entry.hostId, chat_id: entry.chatId } }, 'chat-image', image.id, image.size, fromBase64(image.sha256), image.mediaType)
          uploadId = journal.id
          await store.update(state => { const saved = state.outbox.find(e => e.commandId === entry.commandId)?.images?.find(i => i.id === image.id); if (!saved) throw Error('Image outbox disappeared'); saved.uploadId = uploadId; return null })
          image.uploadId = uploadId
        }
        attachments.push({ label: 'Image', image: await upload(this, uploadId) })
      }
      return { ...entry, draft: { ...entry.draft, attachments } }
    })
    const saved = store.snapshot()
    this.transport = new GrpcTransport(native, `${saved.connection.hostId}_${saved.connection.deviceId}`)
    this.state = { hostId: saved.connection.hostId, phase: 'offline', stale: true, lastSeen: saved.lastSeen, catalog: saved.catalog }
  }
  private mutationGate: Promise<unknown> = Promise.resolve()
  // Online mutations retain receipt identity; only the explicit non-secret chat allowlist retains replayable bodies.
  // An unknown outcome must be queried before another effect can be requested.
  mutate<M extends keyof RpcTypes>(method: M, request: RpcTypes[M]['input'] & { command?: CommandMeta }): Promise<RpcTypes[M]['output']> {
    const operation = this.mutationGate.then(async () => {
      const meta = request.command
      if (!meta || meta.host_id !== this.state.hostId || this.state.phase !== 'ready') throw Error('Mutation requires the connected originating host')
      await this.reconcileMutations()
      if (this.store.snapshot().mutations?.some(m => m.epoch === this.store.snapshot().epoch && !m.receipt?.resolved_by_user && (!m.receipt || ![3, 4, 5, 7].includes(m.receipt.state)))) throw Error('An earlier operation has an unknown outcome. Review operation recovery before retrying.')
      await this.store.update(state => { const records = (state.mutations ?? []).filter(m => !m.receipt || ![3,4,5,7].includes(m.receipt.state)); if (records.length >= 100) throw Error('Operation recovery storage is full'); records.push({ chatRequest: chatRequest(method, request), method, commandId: meta.command_id, epoch: meta.command_epoch }); state.mutations = records; return null })
      const result = await this.transport.unary(method, request)
      const receipt = result as CommandReceipt
      if (receipt.command_id !== meta.command_id || receipt.command_epoch !== meta.command_epoch || receipt.host_id !== meta.host_id) throw Error('Mutation receipt identity mismatch')
      await this.store.update(state => { const record = state.mutations?.find(m => m.commandId === meta.command_id); if (record) record.receipt = receipt; return null })
      return result
    })
    this.mutationGate = operation.catch(() => {}); return operation
  }
  retryChatMutation(id: string): Promise<CommandReceipt> {
    const operation = this.mutationGate.then(() => { if (this.state.phase !== 'ready') throw Error('Connect to the originating host first'); return retryChatMutation(this.store, this.transport, id) })
    this.mutationGate = operation.catch(() => {}); return operation
  }
  async reconcileMutations(): Promise<void> {
    for (const record of this.store.snapshot().mutations ?? []) {
      if (record.receipt && (record.receipt.resolved_by_user || [3,4,5,7].includes(record.receipt.state))) continue
      const receipt = await this.transport.unary('HostService/GetCommand', { host_id: this.state.hostId, command_id: record.commandId, command_epoch: record.epoch })
      if (receipt.command_id !== record.commandId || receipt.command_epoch !== record.epoch || receipt.host_id !== this.state.hostId) throw Error('Recovery receipt identity mismatch')
      await this.store.update(state => { const target = state.mutations?.find(m => m.commandId === record.commandId); if (target) target.receipt = receipt; return null })
    }
  }
  snapshot(): HostSessionState { return this.state }
  subscribe(callback: () => void): () => void { this.listeners.add(callback); return () => { this.listeners.delete(callback) } }
  private publish(patch: Partial<HostSessionState>): void {
    this.state = { ...this.state, ...patch }
    for (const callback of this.listeners) callback()
  }
  async connect(): Promise<void> {
    if (this.opening) {
      await this.opening
      if (!this.disposed && this.foreground && this.state.phase === 'suspended') await this.connect()
      return
    }
    const opening = this.connectNow()
    this.opening = opening
    try { await opening } finally { if (this.opening === opening) this.opening = undefined }
  }
  private async connectNow(): Promise<void> {
    if (this.disposed || !this.foreground) return
    if (this.state.phase === 'connecting' || this.state.phase === 'ready') return
    const generation = ++this.generation
    this.controller?.abort(); this.controller = new AbortController()
    const signal = this.controller.signal
    clearTimeout(this.reconnectTimer)
    this.publish({ phase: 'connecting', stale: true, error: undefined })
    try {
      await this.transport.disconnect()
      const saved = this.store.snapshot(), config = saved.connection
      if (generation !== this.generation) return
      const route = config.routes[config.route]
      if (!route) throw Error('No native route configured')
      const ssh: Record<string, string> = route.kind === 'ssh' ? { sshAddress: route.sshAddress, sshPort: String(route.sshPort), sshUsername: route.username, sshFingerprint: route.fingerprint, sshCredentialRef: route.credentialRef } : {}
      if (generation !== this.generation) return
      await this.transport.open({
        address: route.address, port: String(route.port), serverName: config.serverName,
        hostCertificate: config.hostCertificate, fingerprint: config.fingerprint, credentialRef: config.credentialRef, ...ssh,
      })
      if (generation !== this.generation) { await this.transport.disconnect(); return }
      const info = await this.transport.unary('HostService/GetInfo', { host_id: config.hostId }, signal)
      if (info.host_id !== config.hostId || info.protocol_version !== 1) throw Error('Host identity or protocol mismatch')
      await this.store.update(state => { state.epoch = info.command_epoch; state.lastSeen = new Date().toISOString(); return null })
      if (generation !== this.generation) return
      this.publish({ phase: 'ready', stale: false, info, lastSeen: this.store.snapshot().lastSeen })
      this.attempts = 0
      // Independent bounded RPC streams; a domain storage fault remains a domain status.
      const hostStream = await this.transport.subscribe('HostService/WatchHost', { host_id: config.hostId }, status => {
        if (generation !== this.generation) return
        if (status.host_id !== config.hostId) throw Error('Host status identity mismatch')
        this.publish({ status })
      }, signal)
      void hostStream.done.then(() => this.lost(generation), error => this.lost(generation, error))
      const health = await this.transport.unary('Health/Check', { service: '' }, signal)
      this.publish({ health: health.status })
      const healthStream = await this.transport.subscribe('Health/Watch', { service: '' }, value => {
        if (generation === this.generation) this.publish({ health: value.status })
      }, signal)
      void healthStream.done.then(() => this.lost(generation), error => this.lost(generation, error))
      // Paged snapshots are staged and installed whole; a gap or a delta during a snapshot reconnects.
      const feed = new CatalogFeed(config.hostId)
      const catalogStream = await this.transport.subscribe('CatalogService/WatchSummaries', { host_id: config.hostId }, event => {
        if (generation !== this.generation) return
        const catalog = feed.apply(this.state.catalog, event)
        if (catalog) this.publish({ catalog })
      }, signal)
      void catalogStream.done.then(() => this.lost(generation), error => this.lost(generation, error))
      await this.outbox.reconcile(this.transport)
      await this.reconcileMutations()
    } catch (error) { await this.lost(generation, error) }
  }
  private async lost(generation: number, error?: unknown): Promise<void> {
    if (generation !== this.generation || this.disposed) return
    ++this.generation; this.controller?.abort()
    await this.transport.disconnect()
    if (!this.foreground) return
    this.publish({ phase: 'error', stale: true, error: error instanceof Error ? error.message : 'Connection closed' })
    // Auth/trust/protocol failures require action, not an endless reconnect loop.
    const retryable = !error || error instanceof RpcError && [4, 8, 14].includes(error.code)
    if (retryable && this.attempts < 5) {
      const delay = Math.min(30000, 1000 * 2 ** this.attempts++)
      this.reconnectTimer = setTimeout(() => { void this.connect() }, delay)
    }
  }
  async setForeground(foreground: boolean): Promise<void> {
    this.foreground = foreground
    if (foreground) { this.attempts = 0; await this.connect(); return }
    ++this.generation; clearTimeout(this.reconnectTimer); this.controller?.abort()
    this.publish({ phase: 'suspended', stale: true })
    await this.transport.disconnect()
    await this.store.update(state => { state.catalog = this.state.catalog; return null })
  }
  async selectRoute(index: number): Promise<void> {
    await this.setForeground(false)
    await this.store.update(state => {
      if (!state.connection.routes[index]) throw Error('Route not found')
      state.connection.route = index; return null
    })
    await this.setForeground(true)
  }
  async dispose(): Promise<void> {
    this.disposed = true; ++this.generation; clearTimeout(this.reconnectTimer); this.controller?.abort()
    await this.transport.dispose(); this.listeners.clear()
  }
}
export class RemoteHosts {
  private listeners = new Set<() => void>()
  subscribe(callback: () => void): () => void { this.listeners.add(callback); return () => { this.listeners.delete(callback) } }
  list(): HostSession[] { return [...this.sessions.values()] }
  private publish(): void { for (const listener of this.listeners) listener() }
  private sessions = new Map<string, HostSession>()
  private pending = new Map<string, Promise<HostSession>>()
  private foreground = true
  private native: NativeRemote
  readonly registry: HostRegistry
  constructor(native: NativeRemote) { this.native = native; this.registry = new HostRegistry(native) }
  async restore(): Promise<void> {
    const results = await Promise.allSettled((await this.registry.list()).map(connection => this.add(connection)))
    const failures = results.filter((result): result is PromiseRejectedResult => result.status === 'rejected')
    if (failures.length) throw Error(failures.map(result => String(result.reason)).join('; '))
  }
  add(connection: HostConnection): Promise<HostSession> {
    const existing = this.sessions.get(connection.hostId)
    if (existing) return Promise.resolve(existing)
    const pending = this.pending.get(connection.hostId)
    if (pending) return pending
    const operation = (async () => {
      const store = await HostStore.open(this.native, connection)
      await this.registry.save(connection)
      const session = new HostSession(this.native, store)
      this.sessions.set(connection.hostId, session)
      this.publish()
      await session.setForeground(this.foreground)
      return session
    })().finally(() => this.pending.delete(connection.hostId))
    this.pending.set(connection.hostId, operation)
    return operation
  }
  get(hostId: string): HostSession | undefined { return this.sessions.get(hostId) }
  async setForeground(active: boolean): Promise<void> {
    this.foreground = active
    await Promise.allSettled([...this.sessions.values()].map(session => session.setForeground(active)))
  }
  async cleanupAttachments(): Promise<number> {
    await this.restore() // Any unreadable host state must prevent deletion.
    const retained = new Set<string>()
    const stores = [...this.sessions.values()].map(session => session.store)
    for (const connection of await this.registry.retained()) stores.push(await HostStore.open(this.native, connection))
    for (const store of stores) {
      const state = store.snapshot()
      for (const entry of state.outbox) for (const image of entry.images ?? []) retained.add(image.id)
      for (const draft of Object.values(state.drafts)) for (const image of draft.images ?? []) retained.add(image.id)
      for (const upload of state.uploads ?? []) if (!upload.artifact) retained.add(upload.localId)
    }
    return this.native.cleanupAttachments([...retained])
  }
  async remove(hostId: string): Promise<void> {
    const pending = this.pending.get(hostId); if (pending) await pending
    const session = this.sessions.get(hostId)
    if (session) await session.dispose()
    this.sessions.delete(hostId)
    this.publish()
    await this.registry.remove(hostId)
    // Credential and draft deletion is explicit; removing a connection does not erase recovery data.
  }
}
let hosts: RemoteHosts | undefined
export function remoteHosts(): RemoteHosts { return hosts ??= new RemoteHosts(nativeRemote()) }
