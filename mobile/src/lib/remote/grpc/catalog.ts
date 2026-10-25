import type { Catalog, CatalogSnapshotPart, Cursor, RpcTypes, SummaryEvent } from './schema'
import { RpcError } from './transport'

// The host projects at most 32 MiB of catalog; decoded JSON text is allowed twice that.
const MAX_STAGED = 64 * 1024 * 1024
const sameCursor = (a: Cursor, b: Cursor) => a.runtime_epoch === b.runtime_epoch && a.stream_id === b.stream_id && a.sequence === b.sequence

/** Retryable (resource_exhausted): the client reads the catalog again. */
export class CatalogResync extends RpcError { constructor(message: string) { super(8, message) } }

/** Pages of one snapshot; the catalog is handed out only whole, at its last page. */
export class CatalogStaging {
  private staged?: { boundary: Cursor; catalog: Catalog; bytes: number }
  get staging(): boolean { return !!this.staged }
  discard(): void { this.staged = undefined }
  /** A new first page starts over (the host restarted the snapshot). Any error drops the staging. */
  push(hostId: string, part: CatalogSnapshotPart): { boundary: Cursor; catalog: Catalog } | undefined {
    try { return this.accept(hostId, part) } catch (error) { this.discard(); throw error }
  }
  private accept(hostId: string, part: CatalogSnapshotPart): { boundary: Cursor; catalog: Catalog } | undefined {
    const { boundary, page } = part
    if (!boundary || !page) throw new CatalogResync('Invalid catalog page')
    if (page.host_id !== hostId) throw Error('Catalog identity mismatch')
    if (part.first) this.staged = { boundary, catalog: { host_id: hostId, projects: [], chats: [], terminals: [] }, bytes: 0 }
    const staged = this.staged
    if (!staged) throw new CatalogResync('Catalog page before the first page')
    if (!sameCursor(staged.boundary, boundary)) throw new CatalogResync('Catalog snapshot boundary changed')
    staged.bytes += JSON.stringify(page).length
    if (staged.bytes > MAX_STAGED) throw new CatalogResync('Catalog too large')
    const target = staged.catalog
    for (const project of page.projects) {
      const last = target.projects[target.projects.length - 1]
      if (last?.project_id === project.project_id) {
        // A project continued from the previous page: same metadata, the rest of its checkouts.
        if (last.name !== project.name || last.path !== project.path || last.origin !== project.origin) throw new CatalogResync('Conflicting catalog project page')
        last.checkouts.push(...project.checkouts)
      } else {
        if (target.projects.some(p => p.project_id === project.project_id)) throw new CatalogResync('Repeated catalog project')
        target.projects.push({ ...project, checkouts: [...project.checkouts] })
      }
    }
    target.chats.push(...page.chats)
    target.terminals.push(...page.terminals)
    if (!part.last) return undefined
    this.staged = undefined
    return { boundary: staged.boundary, catalog: target }
  }
}

/** Keeps one catalog current from a WatchSummaries stream: whole snapshots (plain or paged), then deltas in cursor order. */
export class CatalogFeed {
  private staging = new CatalogStaging()
  private cursor?: Cursor
  constructor(private readonly hostId: string) {}
  /** The new catalog, or undefined when it did not change (a staged page, a repeated event). Throws when a resync is required; `current` is never modified. */
  apply(current: Catalog | undefined, event: SummaryEvent): Catalog | undefined {
    const hostId = this.hostId
    if (!event.cursor) throw new CatalogResync('Missing summary cursor')
    if (event.snapshot_part) {
      const whole = this.staging.push(hostId, event.snapshot_part)
      if (!whole) return undefined
      this.cursor = whole.boundary
      return whole.catalog
    }
    if (event.snapshot) {
      if (event.snapshot.host_id !== hostId) throw Error('Catalog identity mismatch')
      this.staging.discard(); this.cursor = event.cursor
      return event.snapshot
    }
    if (this.staging.staging) { this.staging.discard(); throw new CatalogResync('Catalog changed during a snapshot') }
    const cursor = this.cursor
    if (!cursor || cursor.runtime_epoch !== event.cursor.runtime_epoch || cursor.stream_id !== event.cursor.stream_id) throw new CatalogResync('Catalog resync required')
    if (BigInt(event.cursor.sequence) <= BigInt(cursor.sequence)) return undefined
    if (BigInt(event.cursor.sequence) !== BigInt(cursor.sequence) + 1n) throw new CatalogResync('Catalog sequence gap')
    if (!current) throw new CatalogResync('Summary delta before snapshot')
    let next: Catalog
    if (event.chat?.chat) {
      if (event.chat.chat.host_id !== hostId) throw Error('Chat identity mismatch')
      const id = event.chat.chat.chat_id
      next = { ...current, chats: current.chats.filter(c => c.chat?.chat_id !== id).concat([event.chat]) }
    } else if (event.terminal?.terminal) {
      if (event.terminal.terminal.host_id !== hostId) throw Error('Terminal identity mismatch')
      const id = event.terminal.terminal.terminal_id
      next = { ...current, terminals: current.terminals.filter(t => t.terminal?.terminal_id !== id).concat([event.terminal]) }
    } else if (event.deleted?.chat) {
      const id = event.deleted.chat.chat_id
      next = { ...current, chats: current.chats.filter(c => c.chat?.chat_id !== id) }
    } else if (event.deleted?.terminal) {
      const id = event.deleted.terminal.terminal_id
      next = { ...current, terminals: current.terminals.filter(t => t.terminal?.terminal_id !== id) }
    } else throw new CatalogResync('Catalog resync required')
    this.cursor = event.cursor
    return next
  }
}

export interface CatalogStreamClient {
  subscribe(method: 'CatalogService/ReadCatalog', request: RpcTypes['CatalogService/ReadCatalog']['input'], onMessage: (value: SummaryEvent) => void | Promise<void>, signal?: AbortSignal): Promise<{ done: Promise<void>; cancel?(): void }>
}

/** The host's whole catalog, read page by page (ReadCatalog); nothing partial is returned. */
export async function readCatalog(client: CatalogStreamClient, hostId: string, signal?: AbortSignal): Promise<Catalog> {
  const staging = new CatalogStaging()
  let result: Catalog | undefined
  const stream = await client.subscribe('CatalogService/ReadCatalog', { host_id: hostId }, event => {
    if (result) throw new CatalogResync('Catalog event after the last page')
    if (!event.snapshot_part) throw new CatalogResync('Unexpected catalog event')
    result = staging.push(hostId, event.snapshot_part)?.catalog
  }, signal)
  try { await stream.done } catch (error) { staging.discard(); throw error }
  if (!result) throw new CatalogResync('The catalog stream ended before the whole catalog')
  return result
}
