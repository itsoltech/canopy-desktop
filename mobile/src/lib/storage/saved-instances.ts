import * as SecureStore from 'expo-secure-store'

import {
  type SavedInstance,
  type SavedInstanceInput,
  StorageLimitError,
  StoragePayloadTooLargeError,
} from './saved-instances-types'

const STORAGE_KEY = 'canopy.savedInstances.v1'
const MAX_INSTANCES = 8
const PAYLOAD_WARNING_BYTES = 1800

export class SavedInstancesStorage {
  private static listeners = new Set<() => void>()

  static async list(): Promise<SavedInstance[]> {
    return SavedInstancesStorage.readAll()
  }

  static async get(id: string): Promise<SavedInstance | null> {
    const all = await SavedInstancesStorage.readAll()
    return all.find((i) => i.id === id) ?? null
  }

  static async add(input: SavedInstanceInput): Promise<SavedInstance> {
    const all = await SavedInstancesStorage.readAll()

    const duplicate = all.find(
      (i) => i.lanIp === input.lanIp && i.port === input.port && i.token === input.token,
    )
    if (duplicate) {
      const updated: SavedInstance = {
        ...duplicate,
        hostname: input.hostname,
        lastConnectedAt: input.lastConnectedAt ?? duplicate.lastConnectedAt,
      }
      const next = all.map((i) => (i.id === duplicate.id ? updated : i))
      await SavedInstancesStorage.writeAll(next)
      return updated
    }

    if (all.length >= MAX_INSTANCES) {
      throw new StorageLimitError(MAX_INSTANCES)
    }

    const created: SavedInstance = {
      ...input,
      id: makeId(),
      addedAt: new Date().toISOString(),
    }
    const next = [...all, created]
    await SavedInstancesStorage.writeAll(next)
    return created
  }

  static async update(
    id: string,
    patch: Partial<SavedInstanceInput>,
  ): Promise<SavedInstance | null> {
    const all = await SavedInstancesStorage.readAll()
    const target = all.find((i) => i.id === id)
    if (!target) return null

    const updated: SavedInstance = { ...target, ...patch }
    const next = all.map((i) => (i.id === id ? updated : i))
    await SavedInstancesStorage.writeAll(next)
    return updated
  }

  static async remove(id: string): Promise<void> {
    const all = await SavedInstancesStorage.readAll()
    const next = all.filter((i) => i.id !== id)
    if (next.length === all.length) return
    await SavedInstancesStorage.writeAll(next)
  }

  static subscribe(fn: () => void): () => void {
    SavedInstancesStorage.listeners.add(fn)
    return () => {
      SavedInstancesStorage.listeners.delete(fn)
    }
  }

  private static async readAll(): Promise<SavedInstance[]> {
    const raw = await SecureStore.getItemAsync(STORAGE_KEY)
    if (!raw) return []
    try {
      const parsed = JSON.parse(raw)
      if (!Array.isArray(parsed)) return []
      return parsed.filter((v): v is SavedInstance => isValidInstance(v))
    } catch {
      return []
    }
  }

  private static async writeAll(instances: SavedInstance[]): Promise<void> {
    const serialized = JSON.stringify(instances)
    if (serialized.length > PAYLOAD_WARNING_BYTES) {
      throw new StoragePayloadTooLargeError(serialized.length)
    }
    await SecureStore.setItemAsync(STORAGE_KEY, serialized)
    SavedInstancesStorage.notify()
  }

  private static notify(): void {
    for (const fn of SavedInstancesStorage.listeners) fn()
  }
}

function makeId(): string {
  const ts = Date.now().toString(36)
  const rand = Math.random().toString(36).slice(2, 10).padEnd(8, '0')
  return `${ts}-${rand}`
}

function isValidInstance(v: unknown): v is SavedInstance {
  if (!v || typeof v !== 'object') return false
  const o = v as Record<string, unknown>
  return (
    typeof o.id === 'string' &&
    typeof o.nickname === 'string' &&
    typeof o.hostname === 'string' &&
    typeof o.lanIp === 'string' &&
    typeof o.port === 'number' &&
    typeof o.token === 'string' &&
    typeof o.addedAt === 'string'
  )
}
