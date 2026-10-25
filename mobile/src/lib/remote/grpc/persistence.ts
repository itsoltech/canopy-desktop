import type { UploadJournal } from './uploads'
import type { NativeRemote } from './bridge'
import { fromBase64, toBase64 } from './protobuf'
import type { Catalog, CommandReceipt, MessageDraft, SendContext } from './schema'

export type DirectRoute = { kind: 'direct'; address: string; port: number }
export type SshRoute = { kind: 'ssh'; address: string; port: number; sshAddress: string; sshPort: number; username: string; fingerprint: string; credentialRef: string }
export type HostConnection = {
  hostId: string
  deviceId: string
  name: string
  serverName: string
  fingerprint: string
  hostCertificate: string
  credentialRef: string
  routes: (DirectRoute | SshRoute)[]
  route: number
}
export type LocalImage = { id: string; size: number; sha256: string; mediaType: string; uploadId?: string }
export type OutboxEntry = {
  images?: LocalImage[]
  commandId: string
  messageId: string
  hostId: string
  deviceId: string
  chatId: string
  context: SendContext
  draft: MessageDraft
  createdAt: string
  epoch?: string
  receipt?: CommandReceipt
}
export type HostState = {
  version: 1
  connection: HostConnection
  epoch?: string
  closingEpoch?: string
  mutations?: { chatRequest?: string; method: string; commandId: string; epoch: string; receipt?: CommandReceipt }[]
  uploads?: UploadJournal[]
  outbox: OutboxEntry[]
  drafts: Record<string, { revision?: string; context: SendContext; draft: MessageDraft; images?: LocalImage[] }>
  catalog?: Catalog
  lastSeen?: string
  edits?: Record<string, { preview: import('./schema').EditPreview; draft: MessageDraft; restoreFiles: boolean }>
  taskDrafts?: Record<string, Record<string, unknown>>
  views?: Record<string, { scroll?: number; text?: string; baseHash?: string; newline?: 'lf' | 'crlf' }>
}
export interface StateIO { readState(key: string): Promise<string | null>; writeState(key: string, json: string): Promise<void> }
const serialize = (value: unknown) => JSON.stringify(value, (_, value) => value instanceof Uint8Array ? { $canopyBytes: toBase64(value) } : value)
const parse = (value: string) => JSON.parse(value, (_, value) => value && typeof value === 'object' && typeof value.$canopyBytes === 'string' && Object.keys(value).length === 1 ? fromBase64(value.$canopyBytes) : value)
const clone = <T>(value: T): T => value === undefined ? value : parse(serialize(value))
function key(hostId: string, deviceId: string): string {
  // Host/device IDs are UUIDs; credentials/outbox are never adopted into another identity.
  if (!/^[a-zA-Z0-9_-]{1,60}$/.test(hostId) || !/^[a-zA-Z0-9_-]{1,60}$/.test(deviceId)) throw Error('invalid host/device identity')
  return `${hostId}_${deviceId}`
}
export class HostStore {
  private listeners = new Set<() => void>()
  subscribe(callback: () => void): () => void { this.listeners.add(callback); return () => { this.listeners.delete(callback) } }
  private tail: Promise<unknown> = Promise.resolve()
  private state: HostState
  readonly io: StateIO
  private readonly key: string
  private constructor(io: StateIO, state: HostState) { this.io = io; this.state = state; this.key = key(state.connection.hostId, state.connection.deviceId) }
  static async open(io: StateIO, connection: HostConnection): Promise<HostStore> {
    const raw = await io.readState(key(connection.hostId, connection.deviceId))
    let state: HostState
    if (raw) {
      state = parse(raw)
      if (state.version !== 1 || state.connection?.hostId !== connection.hostId || state.connection?.deviceId !== connection.deviceId || !Array.isArray(state.outbox) || !state.drafts) throw Error('Invalid saved host state; data retained')
      if (state.connection.fingerprint !== connection.fingerprint || state.connection.credentialRef !== connection.credentialRef) throw Error('Saved host identity differs; explicit pairing required')
    } else {
      state = { version: 1, connection: clone(connection), outbox: [], drafts: {} }
      await io.writeState(key(connection.hostId, connection.deviceId), serialize(state))
    }
    return new HostStore(io, state)
  }
  snapshot(): HostState { return clone(this.state) }
  update<T>(change: (state: HostState) => T): Promise<T> {
    const operation = this.tail.then(async () => {
      const next = clone(this.state)
      const result = change(next)
      if (next.outbox.length > 100) throw Error('Outbox is full; draft preserved')
      const json = serialize(next)
      if (new TextEncoder().encode(json).length > 64 * 1024 * 1024) throw Error('Local text storage is full; previous data preserved')
      await this.io.writeState(this.key, json)
      this.state = next
      for (const listener of this.listeners) listener()
      return clone(result)
    })
    this.tail = operation.catch(() => {})
    return operation
  }
}
export class HostRegistry {
  private tail: Promise<unknown> = Promise.resolve()
  private readonly io: Pick<NativeRemote, 'readState' | 'writeState'>
  constructor(io: Pick<NativeRemote, 'readState' | 'writeState'>) { this.io = io }
  async list(): Promise<HostConnection[]> {
    const raw = await this.io.readState('hosts')
    if (!raw) return []
    const data = parse(raw)
    if (data.version !== 1 || !Array.isArray(data.hosts)) throw Error('Invalid host registry; data retained')
    return data.hosts
  }
  save(connection: HostConnection): Promise<void> {
    key(connection.hostId, connection.deviceId)
    const operation = this.tail.then(async () => {
      const hosts = await this.list()
      const existing = hosts.find(h => h.hostId === connection.hostId)
      if (existing && (existing.deviceId !== connection.deviceId || existing.fingerprint !== connection.fingerprint)) throw Error('Remove old trust before pairing another identity')
      const next = hosts.filter(h => h.hostId !== connection.hostId).concat([connection])
      if (next.length > 8) throw Error('Host limit reached')
      await this.io.writeState('hosts', serialize({ version: 1, hosts: next }))
    })
    this.tail = operation.catch(() => {})
    return operation
  }
  async retained(): Promise<HostConnection[]> {
    const raw = await this.io.readState('retained-hosts')
    if (!raw) return []
    const data = parse(raw)
    if (data.version !== 1 || !Array.isArray(data.hosts)) throw Error('Invalid recovery registry; local attachments retained')
    return data.hosts
  }
  remove(hostId: string): Promise<void> {
    const operation = this.tail.then(async () => {
      const current = await this.list()
      const removed = current.find(h => h.hostId === hostId)
      if (removed) {
        const retained = await this.retained()
        if (!retained.some(h => h.hostId === removed.hostId && h.deviceId === removed.deviceId)) retained.push(removed)
        await this.io.writeState('retained-hosts', serialize({ version: 1, hosts: retained }))
      }
      const hosts = current.filter(h => h.hostId !== hostId)
      // Retain host-local drafts/outbox for explicit recovery, never reassign them.
      await this.io.writeState('hosts', serialize({ version: 1, hosts }))
    })
    this.tail = operation.catch(() => {})
    return operation
  }
}
