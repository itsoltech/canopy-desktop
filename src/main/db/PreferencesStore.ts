import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'

export class PreferencesStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  get(key: string): string | null {
    const row = this.db.prepare('SELECT value FROM preferences WHERE key = ?').get(key) as
      | { value: string }
      | undefined
    return row?.value ?? null
  }

  set(key: string, value: string): void {
    this.db.prepare('INSERT OR REPLACE INTO preferences (key, value) VALUES (?, ?)').run(key, value)
  }

  getAll(): Record<string, string> {
    const rows = this.db.prepare('SELECT key, value FROM preferences').all() as {
      key: string
      value: string
    }[]
    const result: Record<string, string> = {}
    for (const row of rows) {
      result[row.key] = row.value
    }
    return result
  }

  delete(key: string): void {
    this.db.prepare('DELETE FROM preferences WHERE key = ?').run(key)
  }
}
