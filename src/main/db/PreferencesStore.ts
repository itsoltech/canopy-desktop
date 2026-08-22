import { safeStorage } from 'electron'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'
import {
  isEncryptedPreferenceKey,
  isExportablePreferenceKey,
  isMainProcessOnlyPreferenceKey,
} from './preferenceKeys'

export class PreferencesStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  get(key: string): string | null {
    const row = this.db.prepare('SELECT value FROM preferences WHERE key = ?').get(key) as
      { value: string } | undefined
    if (!row) return null
    if (isEncryptedPreferenceKey(key) && safeStorage.isEncryptionAvailable()) {
      try {
        return safeStorage.decryptString(Buffer.from(row.value, 'base64'))
      } catch {
        // Fallback: value stored before encryption was enabled
        return row.value
      }
    }
    return row.value
  }

  isEncrypted(key: string): boolean {
    return isEncryptedPreferenceKey(key)
  }

  isMainProcessOnly(key: string): boolean {
    return isMainProcessOnlyPreferenceKey(key)
  }

  /** Keys only — values stay in the store, so this is safe for secret-class key enumeration. */
  keysWithPrefix(prefix: string): string[] {
    const rows = this.db
      .prepare("SELECT key FROM preferences WHERE key LIKE ? || '%'")
      .all(prefix) as { key: string }[]
    return rows.map((r) => r.key)
  }

  /**
   * Encrypt an encrypted-class key's value with this machine's safeStorage,
   * falling back to plaintext only when no OS keyring is available. The
   * fallback is logged so it is observable (mirrors CredentialStore) rather
   * than silently persisting a secret in cleartext.
   */
  private encryptForStorage(key: string, value: string): string {
    if (!isEncryptedPreferenceKey(key)) return value
    if (safeStorage.isEncryptionAvailable()) {
      return safeStorage.encryptString(value).toString('base64')
    }
    console.warn(
      `[PreferencesStore] safeStorage encryption unavailable — "${key}" stored without OS-level encryption. Configure a system keyring for secure storage.`,
    )
    return value
  }

  set(key: string, value: string): void {
    const stored = this.encryptForStorage(key, value)
    this.db
      .prepare('INSERT OR REPLACE INTO preferences (key, value) VALUES (?, ?)')
      .run(key, stored)
  }

  /**
   * Runs synchronous preference mutations as one SQLite transaction. Callers keep using
   * `set`/`delete`, so encrypted-key handling stays centralized in this store while a failure in
   * any later mutation rolls the earlier ones back.
   */
  runInTransaction<T>(operation: () => T): T {
    if (this.db.inTransaction) return operation()
    return this.db.transaction(operation)()
  }

  getAll(): Record<string, string> {
    const rows = this.db.prepare('SELECT key, value FROM preferences').all() as {
      key: string
      value: string
    }[]
    const result: Record<string, string> = {}
    for (const row of rows) {
      if (isMainProcessOnlyPreferenceKey(row.key)) continue

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
      if (!isExportablePreferenceKey(row.key)) continue
      if (isEncryptedPreferenceKey(row.key) && safeStorage.isEncryptionAvailable()) {
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
      if (!isExportablePreferenceKey(key)) continue
      const stored = this.encryptForStorage(key, value)
      stmt.run(key, stored)
      count++
    }
    return count
  }

  delete(key: string): void {
    this.db.prepare('DELETE FROM preferences WHERE key = ?').run(key)
  }
}
