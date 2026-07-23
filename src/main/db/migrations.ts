export interface Migration {
  id: number
  up: string
}

/**
 * Migrations are built per-platform. On Windows (`windowsPaths`), path comparison
 * keys fold separators AND case — `C:\Source` and `c:/source` are one directory
 * there. On POSIX both conversions are wrong (backslash is a legal filename
 * character and filesystems are case-sensitive), so paths are compared verbatim and
 * never rewritten. Exposed as a function so tests can exercise both variants
 * against a real SQLite database (node:sqlite).
 */
export function buildMigrations(windowsPaths: boolean): Migration[] {
  // Stored form of a path column: forward slashes on Windows, untouched on POSIX.
  const normExpr = (col: string): string => (windowsPaths ? `REPLACE(${col}, '\\', '/')` : col)
  // Comparison key: on Windows, the `canopy_path_key` SQL function registered by
  // Database — it delegates to comparableWorkspacePath(), so migration, store
  // lookups, and in-memory maps share ONE Unicode-aware folding rule (SQLite's own
  // LOWER()/NOCASE fold ASCII only and would split e.g. C:/Users/Łukasz from
  // c:/users/łukasz). On POSIX paths compare verbatim.
  const pathKey = (col: string): string => (windowsPaths ? `canopy_path_key(${col})` : col)

  return [
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
      // Workspace paths were persisted in whatever separator style (and, on Windows,
      // letter case) they arrived in — native dialogs → C:\..., git/renderer → C:/...
      // — producing duplicate workspace rows for the same directory. Layouts stranded
      // under the stale duplicate could never be deleted again and re-spawned ghost
      // windows on every launch. Merge duplicates by comparison key (keeping the most
      // recently opened row, preserving its casing), rebuild layouts keeping exactly
      // one row per (canonical workspace, worktree key) — ranked by updated_at DESC,
      // rowid DESC, the tie-break LayoutStore's one-second timestamps need — and
      // canonicalize separators before the stores start enforcing the canonical form
      // at runtime. On POSIX the keys are verbatim, so distinct directories (including
      // ones with literal backslashes in their names) are never merged or rewritten.
      id: 11,
      up: `
      CREATE TEMP TABLE ws_canon AS
      SELECT id,
             FIRST_VALUE(id) OVER (
               PARTITION BY ${pathKey('path')}
               ORDER BY COALESCE(last_opened, '') DESC, rowid DESC
             ) AS keep_id
      FROM workspaces;

      CREATE TEMP TABLE layout_canon AS
      SELECT c.keep_id AS workspace_id,
             ${normExpr('l.worktree_path')} AS worktree_path,
             l.layout_json,
             l.updated_at,
             ROW_NUMBER() OVER (
               PARTITION BY c.keep_id, ${pathKey('l.worktree_path')}
               ORDER BY l.updated_at DESC, l.rowid DESC
             ) AS rn
      FROM workspace_layouts l JOIN ws_canon c ON c.id = l.workspace_id;

      DELETE FROM workspace_layouts;
      INSERT INTO workspace_layouts (workspace_id, worktree_path, layout_json, updated_at)
        SELECT workspace_id, worktree_path, layout_json, updated_at
        FROM layout_canon WHERE rn = 1;
      DROP TABLE layout_canon;

      UPDATE skill_definitions
        SET workspace_id = (SELECT keep_id FROM ws_canon WHERE ws_canon.id = skill_definitions.workspace_id)
        WHERE workspace_id IN (SELECT id FROM ws_canon WHERE id != keep_id);

      DELETE FROM workspaces WHERE id IN (SELECT id FROM ws_canon WHERE id != keep_id);
      ${windowsPaths ? `UPDATE workspaces SET path = REPLACE(path, '\\', '/');` : ''}
      DROP TABLE ws_canon;
    `,
    },
  ]
}
