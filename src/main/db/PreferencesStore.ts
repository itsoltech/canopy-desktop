import { safeStorage } from 'electron'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'

const ENCRYPTED_KEYS = new Set(['claude.apiKey', 'gemini.apiKey'])
const ENCRYPTED_KEY_PREFIXES = ['issueTracker.token.']

function isEncryptedKey(key: string): boolean {
  if (ENCRYPTED_KEYS.has(key)) return true
  return ENCRYPTED_KEY_PREFIXES.some((prefix) => key.startsWith(prefix))
}

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
    if (isEncryptedKey(key) && safeStorage.isEncryptionAvailable()) {
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
      isEncryptedKey(key) && safeStorage.isEncryptionAvailable()
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
      if (isEncryptedKey(row.key) && safeStorage.isEncryptionAvailable()) {
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
