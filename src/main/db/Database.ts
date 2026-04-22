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
  {
    id: 4,
    up: `
      DELETE FROM tool_definitions WHERE id IN ('htop', 'btop');
      UPDATE tool_definitions SET icon = 'ClaudeAI' WHERE id = 'claude';
      UPDATE tool_definitions SET icon = 'OpenAI' WHERE id = 'codex';
      UPDATE tool_definitions SET icon = 'Gemini' WHERE id = 'gemini';
      UPDATE tool_definitions SET icon = 'Git' WHERE id = 'lazygit';
      UPDATE tool_definitions SET icon = 'terminal' WHERE id = 'shell';
    `,
  },
  {
    id: 5,
    up: `
      INSERT OR IGNORE INTO tool_definitions (id, name, command, args_json, icon, category, is_custom)
      VALUES ('browser', 'Browser', 'browser', '[]', 'Globe', 'browser', 0);
    `,
  },
  {
    id: 6,
    up: `
      CREATE TABLE IF NOT EXISTS credentials (
        id TEXT PRIMARY KEY,
        domain TEXT NOT NULL,
        username TEXT NOT NULL,
        password_enc TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE UNIQUE INDEX IF NOT EXISTS idx_credentials_domain_user ON credentials(domain, username);
    `,
  },
  {
    id: 7,
    up: `
      ALTER TABLE credentials ADD COLUMN title TEXT NOT NULL DEFAULT '';
    `,
  },
  {
    id: 8,
    up: `
      CREATE TABLE IF NOT EXISTS onboarding_completions (
        step_id TEXT PRIMARY KEY,
        completed_at TEXT NOT NULL DEFAULT (datetime('now')),
        app_version TEXT NOT NULL
      );
    `,
  },
  {
    id: 9,
    up: `
      CREATE TABLE IF NOT EXISTS agent_profiles (
        id          TEXT PRIMARY KEY,
        agent_type  TEXT NOT NULL,
        name        TEXT NOT NULL,
        is_default  INTEGER NOT NULL DEFAULT 0,
        sort_index  INTEGER NOT NULL DEFAULT 0,
        prefs_json  TEXT NOT NULL DEFAULT '{}',
        api_key_enc TEXT,
        created_at  TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
      );
      CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_profiles_type_name
        ON agent_profiles(agent_type, name);
      CREATE INDEX IF NOT EXISTS idx_agent_profiles_type_sort
        ON agent_profiles(agent_type, sort_index);
    `,
  },
  {
    id: 10,
    up: `
      CREATE TABLE IF NOT EXISTS skill_definitions (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        version TEXT NOT NULL DEFAULT '1.0.0',
        prompt TEXT NOT NULL,
        agents_json TEXT NOT NULL DEFAULT '[]',
        metadata_json TEXT NOT NULL DEFAULT '{}',
        source_type TEXT NOT NULL,
        source_uri TEXT NOT NULL,
        install_method TEXT NOT NULL DEFAULT 'copy',
        scope TEXT NOT NULL DEFAULT 'project',
        workspace_id TEXT,
        enabled_agents_json TEXT NOT NULL DEFAULT '[]',
        installed_at TEXT NOT NULL DEFAULT (datetime('now')),
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
      );
    `,
  },
  {
    id: 11,
    up: `
      CREATE TABLE IF NOT EXISTS conversations (
        id                TEXT PRIMARY KEY,
        workspace_id      TEXT NOT NULL,
        worktree_path     TEXT NOT NULL,
        agent_profile_id  TEXT NOT NULL,
        sdk_session_id    TEXT,
        title             TEXT,
        model             TEXT NOT NULL,
        permission_mode   TEXT NOT NULL,
        status            TEXT NOT NULL DEFAULT 'active',
        created_at        TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
        FOREIGN KEY (agent_profile_id) REFERENCES agent_profiles(id) ON DELETE RESTRICT
      );
      CREATE INDEX IF NOT EXISTS idx_conversations_workspace
        ON conversations(workspace_id, updated_at DESC);

      CREATE TABLE IF NOT EXISTS sdk_messages (
        id              TEXT PRIMARY KEY,
        conversation_id TEXT NOT NULL,
        role            TEXT NOT NULL,
        content         TEXT NOT NULL,
        content_json    TEXT NOT NULL,
        tool_calls_json TEXT,
        tokens_in       INTEGER,
        tokens_out      INTEGER,
        created_at      TEXT NOT NULL DEFAULT (datetime('now')),
        FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
      );
      CREATE INDEX IF NOT EXISTS idx_sdk_messages_conversation
        ON sdk_messages(conversation_id, created_at);

      CREATE TABLE IF NOT EXISTS sdk_tool_events (
        id              TEXT PRIMARY KEY,
        message_id      TEXT NOT NULL,
        conversation_id TEXT NOT NULL,
        tool_name       TEXT NOT NULL,
        input_json      TEXT NOT NULL,
        result_text     TEXT,
        is_error        INTEGER NOT NULL DEFAULT 0,
        decision        TEXT,
        duration_ms     INTEGER,
        created_at      TEXT NOT NULL DEFAULT (datetime('now')),
        FOREIGN KEY (message_id) REFERENCES sdk_messages(id) ON DELETE CASCADE,
        FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
      );
      CREATE INDEX IF NOT EXISTS idx_sdk_tool_events_message
        ON sdk_tool_events(message_id, created_at);

      CREATE TABLE IF NOT EXISTS sdk_attachments (
        id              TEXT PRIMARY KEY,
        message_id      TEXT NOT NULL,
        conversation_id TEXT NOT NULL,
        kind            TEXT NOT NULL,
        filename        TEXT NOT NULL,
        path            TEXT NOT NULL,
        mime_type       TEXT NOT NULL,
        size_bytes      INTEGER NOT NULL,
        created_at      TEXT NOT NULL DEFAULT (datetime('now')),
        FOREIGN KEY (message_id) REFERENCES sdk_messages(id) ON DELETE CASCADE,
        FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
      );
      CREATE INDEX IF NOT EXISTS idx_sdk_attachments_conversation
        ON sdk_attachments(conversation_id);
    `,
  },
  {
    id: 12,
    up: `
      CREATE VIRTUAL TABLE IF NOT EXISTS sdk_messages_fts USING fts5(
        content,
        conversation_id UNINDEXED,
        created_at UNINDEXED,
        content='sdk_messages',
        content_rowid='rowid'
      );

      CREATE TRIGGER IF NOT EXISTS sdk_messages_ai AFTER INSERT ON sdk_messages BEGIN
        INSERT INTO sdk_messages_fts(rowid, content, conversation_id, created_at)
        VALUES (new.rowid, new.content, new.conversation_id, new.created_at);
      END;

      CREATE TRIGGER IF NOT EXISTS sdk_messages_ad AFTER DELETE ON sdk_messages BEGIN
        INSERT INTO sdk_messages_fts(sdk_messages_fts, rowid, content)
        VALUES ('delete', old.rowid, old.content);
      END;

      CREATE TRIGGER IF NOT EXISTS sdk_messages_au AFTER UPDATE ON sdk_messages BEGIN
        INSERT INTO sdk_messages_fts(sdk_messages_fts, rowid, content)
        VALUES ('delete', old.rowid, old.content);
        INSERT INTO sdk_messages_fts(rowid, content, conversation_id, created_at)
        VALUES (new.rowid, new.content, new.conversation_id, new.created_at);
      END;
    `,
  },
  {
    id: 13,
    up: `
      INSERT OR IGNORE INTO tool_definitions (id, name, command, args_json, icon, category, is_custom)
      VALUES ('claude-sdk', 'Claude (SDK)', 'sdkchat:internal', '[]', 'ClaudeAI', 'ai', 0);
    `,
  },
  {
    id: 14,
    up: `
      ALTER TABLE sdk_tool_events ADD COLUMN answers_json TEXT;
    `,
  },
  {
    id: 15,
    up: `
      ALTER TABLE sdk_messages ADD COLUMN model TEXT;
    `,
  },
  {
    id: 16,
    up: `
      ALTER TABLE sdk_messages ADD COLUMN thinking TEXT NOT NULL DEFAULT '';
    `,
  },
  {
    id: 17,
    up: `
      ALTER TABLE sdk_messages ADD COLUMN parent_subagent_id TEXT;
      ALTER TABLE sdk_tool_events ADD COLUMN parent_subagent_id TEXT;
    `,
  },
  {
    id: 18,
    up: `
      ALTER TABLE conversations ADD COLUMN effort_level TEXT;
    `,
  },
]

export class Database {
  readonly db: BetterSqlite3Database
  private closed = false

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
    if (this.closed) return
    this.closed = true
    this.db.close()
  }

  isClosed(): boolean {
    return this.closed
  }
}
