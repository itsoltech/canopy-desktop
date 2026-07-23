import BetterSqlite3 from 'better-sqlite3'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import { app } from 'electron'
import { join } from 'path'
import { mkdirSync } from 'fs'
import { buildMigrations } from './migrations'
import { comparableWorkspacePath } from './workspacePaths'

export class Database {
  readonly db: BetterSqlite3Database
  private closed = false

  constructor() {
    const dbPath = this.getDbPath()
    mkdirSync(join(dbPath, '..'), { recursive: true })

    this.db = new BetterSqlite3(dbPath)
    this.db.pragma('journal_mode = WAL')
    this.db.pragma('foreign_keys = ON')

    // Single source of truth for path identity in SQL. SQLite's built-in LOWER() and
    // NOCASE fold ASCII only, while comparableWorkspacePath() folds full Unicode
    // (C:/Users/Łukasz ≡ c:/users/łukasz on Windows) — so migration 11 and the store
    // lookups all call this one JS implementation instead of mixing folding rules.
    this.db.function('canopy_path_key', { deterministic: true }, (path) =>
      comparableWorkspacePath(String(path)),
    )

    this.runMigrations()
  }

  private getDbPath(): string {
    return join(app.getPath('userData'), 'canopy.db')
  }

  private runMigrations(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS _migrations (
        id INTEGER PRIMARY KEY,
        applied_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `)

    const applied = new Set(
      this.db
        .prepare('SELECT id FROM _migrations')
        .all()
        .map((row) => (row as { id: number }).id),
    )

    for (const migration of buildMigrations(process.platform === 'win32')) {
      if (applied.has(migration.id)) continue

      const run = this.db.transaction(() => {
        this.db.exec(migration.up)
        this.db.prepare('INSERT INTO _migrations (id) VALUES (?)').run(migration.id)
      })
      run()
    }
  }

  close(): void {
    if (this.closed) return
    this.closed = true
    this.db.close()
  }

  isClosed(): boolean {
    return this.closed
  }
}
