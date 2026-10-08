-- Schema-only fixture from Electron canopy.db, migrations 1-11. No user rows.
CREATE TABLE _migrations (
        id INTEGER PRIMARY KEY,
        applied_at TEXT NOT NULL DEFAULT (datetime('now'))
      );
CREATE TABLE agent_profiles (
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
CREATE TABLE credentials (
        id TEXT PRIMARY KEY,
        domain TEXT NOT NULL,
        username TEXT NOT NULL,
        password_enc TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      , title TEXT NOT NULL DEFAULT '');
CREATE TABLE onboarding_completions (
        step_id TEXT PRIMARY KEY,
        completed_at TEXT NOT NULL DEFAULT (datetime('now')),
        app_version TEXT NOT NULL
      );
CREATE TABLE preferences (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );
CREATE TABLE skill_definitions (
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
CREATE TABLE tool_definitions (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        command TEXT NOT NULL,
        args_json TEXT NOT NULL DEFAULT '[]',
        icon TEXT NOT NULL DEFAULT 'terminal',
        category TEXT NOT NULL DEFAULT 'system',
        is_custom INTEGER NOT NULL DEFAULT 0
      );
CREATE TABLE workspace_layouts (
        workspace_id TEXT NOT NULL,
        worktree_path TEXT NOT NULL,
        layout_json TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (workspace_id, worktree_path),
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
      );
CREATE TABLE workspaces (
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
CREATE UNIQUE INDEX idx_agent_profiles_type_name
        ON agent_profiles(agent_type, name);
CREATE INDEX idx_agent_profiles_type_sort
        ON agent_profiles(agent_type, sort_index);
CREATE UNIQUE INDEX idx_credentials_domain_user ON credentials(domain, username);
INSERT INTO _migrations(id,applied_at) VALUES(1,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(2,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(3,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(4,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(5,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(6,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(7,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(8,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(9,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(10,'2026-01-01 00:00:00');
INSERT INTO _migrations(id,applied_at) VALUES(11,'2026-01-01 00:00:00');
