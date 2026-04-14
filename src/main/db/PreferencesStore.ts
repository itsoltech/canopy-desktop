import { safeStorage } from 'electron'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'

const ENCRYPTED_KEYS = new Set(['claude.apiKey', 'gemini.apiKey', 'opencode.apiKey'])
const ENCRYPTED_KEY_PREFIXES = ['taskTracker.token.']

/**
 * Preference keys that are bound to this specific machine or represent
 * transient runtime state. They must never be written to a settings export,
 * because restoring them on another machine would corrupt local state
 * (orphan workspace IDs, stale window geometry, wrong device identity, etc).
 */
const NON_EXPORTABLE_KEYS = new Set([
  'app.lastSeenVersion',
  'openWindowConfigs',
  'telemetry.lastPingDate',
  'remote.lastPort',
  'remote.trustedDevices',
  'taskTracker.migratedToGlobalConfig',
])

const NON_EXPORTABLE_PREFIXES = ['workspace:']

function isEncryptedKey(key: string): boolean {
  if (ENCRYPTED_KEYS.has(key)) return true
  return ENCRYPTED_KEY_PREFIXES.some((prefix) => key.startsWith(prefix))
}

function isExportableKey(key: string): boolean {
  if (NON_EXPORTABLE_KEYS.has(key)) return false
  return !NON_EXPORTABLE_PREFIXES.some((prefix) => key.startsWith(prefix))
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
      if (isEncryptedKey(row.key)) continue

      result[row.key] = row.value
    }
    return result
  }

  /**
   * Main-process-only: returns every exportable preference including
   * encrypted keys, decrypted to plaintext. Machine-bound and runtime
   * state keys (window geometry, device identity, version tracking, etc)
   * are filtered out. Never expose via IPC — used for settings export.
   */
  getAllDecrypted(): Record<string, string> {
    const rows = this.db.prepare('SELECT key, value FROM preferences').all() as {
      key: string
      value: string
    }[]
    const result: Record<string, string> = {}
    for (const row of rows) {
      if (!isExportableKey(row.key)) continue
      if (isEncryptedKey(row.key) && safeStorage.isEncryptionAvailable()) {
        try {
          result[row.key] = safeStorage.decryptString(Buffer.from(row.value, 'base64'))
          continue
        } catch {
          // Value was stored before encryption; fall through to raw value
        }
      }
      result[row.key] = row.value
    }
    return result
  }

  /**
   * Bulk upsert for settings import. Re-encrypts known encrypted keys with
   * this machine's safeStorage. Non-exportable keys are silently skipped as
   * a defense in depth — if an older export file contains them, they must
   * not be allowed to overwrite destination-local state. Runs no internal
   * transaction — the caller (SettingsExportService) wraps this and sibling
   * calls in one outer transaction so a partial import rolls back atomically.
   */
  setMany(entries: Record<string, string>): number {
    const stmt = this.db.prepare('INSERT OR REPLACE INTO preferences (key, value) VALUES (?, ?)')
    let count = 0
    for (const [key, value] of Object.entries(entries)) {
      if (!isExportableKey(key)) continue
      const stored =
        isEncryptedKey(key) && safeStorage.isEncryptionAvailable()
          ? safeStorage.encryptString(value).toString('base64')
          : value
      stmt.run(key, stored)
      count++
    }
    return count
  }

  delete(key: string): void {
    this.db.prepare('DELETE FROM preferences WHERE key = ?').run(key)
  }
}
