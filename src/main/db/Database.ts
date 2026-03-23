import BetterSqlite3 from 'better-sqlite3'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import { app } from 'electron'
import { join } from 'path'
import { mkdirSync } from 'fs'

interface Migration {
  id: number
  up: string
}

const migrations: Migration[] = [
  {
    id: 1,
    up: `
      CREATE TABLE IF NOT EXISTS workspaces (
        id TEXT PRIMARY KEY,
        path TEXT UNIQUE NOT NULL,
        name TEXT NOT NULL,
        is_git_repo INTEGER NOT NULL DEFAULT 0,
        last_opened TEXT,
        cached_branch TEXT,
        cached_dirty INTEGER,
        cached_ahead_behind TEXT,
        cached_worktree_count INTEGER
      );

      CREATE TABLE IF NOT EXISTS tool_definitions (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        command TEXT NOT NULL,
        args_json TEXT NOT NULL DEFAULT '[]',
        icon TEXT NOT NULL DEFAULT 'terminal',
        category TEXT NOT NULL DEFAULT 'system',
        is_custom INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS preferences (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );
    `,
  },
  {
    id: 2,
    up: `
      INSERT OR IGNORE INTO tool_definitions (id, name, command, args_json, icon, category, is_custom) VALUES
        ('claude', 'Claude Code', 'claude', '[]', 'brain', 'ai', 0),
        ('codex', 'Codex', 'codex', '[]', 'sparkles', 'ai', 0),
        ('gemini', 'Gemini CLI', 'gemini', '[]', 'wand', 'ai', 0),
        ('opencode', 'OpenCode', 'opencode', '[]', 'code', 'ai', 0),
        ('lazygit', 'LazyGit', 'lazygit', '[]', 'git-branch', 'git', 0),
        ('htop', 'htop', 'htop', '[]', 'activity', 'system', 0),
        ('btop', 'btop', 'btop', '[]', 'bar-chart', 'system', 0),
        ('shell', 'Shell', 'shell', '[]', 'terminal', 'shell', 0);
    `,
  },
  {
    id: 3,
    up: `
      CREATE TABLE IF NOT EXISTS workspace_layouts (
        workspace_id TEXT NOT NULL,
        worktree_path TEXT NOT NULL,
        layout_json TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (workspace_id, worktree_path),
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
      );
    `,
  },
]

export class Database {
  readonly db: BetterSqlite3Database

  constructor() {
    const dbPath = this.getDbPath()
    mkdirSync(join(dbPath, '..'), { recursive: true })

    this.db = new BetterSqlite3(dbPath)
    this.db.pragma('journal_mode = WAL')
    this.db.pragma('foreign_keys = ON')

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

    for (const migration of migrations) {
      if (applied.has(migration.id)) continue

      const run = this.db.transaction(() => {
        this.db.exec(migration.up)
        this.db.prepare('INSERT INTO _migrations (id) VALUES (?)').run(migration.id)
      })
      run()
    }
  }

  close(): void {
    this.db.close()
  }
}
