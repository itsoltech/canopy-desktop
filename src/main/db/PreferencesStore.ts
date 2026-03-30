import { safeStorage } from 'electron'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'

const ENCRYPTED_KEYS = new Set(['claude.apiKey', 'gemini.apiKey'])

export class PreferencesStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  get(key: string): string | null {
    const row = this.db.prepare('SELECT value FROM preferences WHERE key = ?').get(key) as
      | { value: string }
      | undefined
    if (!row) return null
    if (ENCRYPTED_KEYS.has(key) && safeStorage.isEncryptionAvailable()) {
      try {
        return safeStorage.decryptString(Buffer.from(row.value, 'base64'))
      } catch {
        // Fallback: value stored before encryption was enabled
        return row.value
      }
    }
    return row.value
  }

  set(key: string, value: string): void {
    const stored =
      ENCRYPTED_KEYS.has(key) && safeStorage.isEncryptionAvailable()
        ? safeStorage.encryptString(value).toString('base64')
        : value
    this.db
      .prepare('INSERT OR REPLACE INTO preferences (key, value) VALUES (?, ?)')
      .run(key, stored)
  }

  getAll(): Record<string, string> {
    const rows = this.db.prepare('SELECT key, value FROM preferences').all() as {
      key: string
      value: string
    }[]
    const result: Record<string, string> = {}
    for (const row of rows) {
      if (ENCRYPTED_KEYS.has(row.key) && safeStorage.isEncryptionAvailable()) {
        try {
          result[row.key] = safeStorage.decryptString(Buffer.from(row.value, 'base64'))
          continue
        } catch {
          // Fallback: unencrypted legacy value
        }
      }
      result[row.key] = row.value
    }
    return result
  }

  delete(key: string): void {
    this.db.prepare('DELETE FROM preferences WHERE key = ?').run(key)
  }
}
