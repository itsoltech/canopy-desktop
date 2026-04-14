export type SavedInstance = {
  id: string
  nickname: string
  hostname: string
  lanIp: string
  port: number
  token: string
  addedAt: string
  lastConnectedAt?: string
}

export type SavedInstanceInput = Omit<SavedInstance, 'id' | 'addedAt'>

export class StorageLimitError extends Error {
  constructor(limit: number) {
    super(`Saved instances limit reached (max ${limit})`)
    this.name = 'StorageLimitError'
  }
}

export class StoragePayloadTooLargeError extends Error {
  constructor(bytes: number) {
    super(`Saved instances payload too large (${bytes} bytes)`)
    this.name = 'StoragePayloadTooLargeError'
  }
}
