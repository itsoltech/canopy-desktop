import { describe, expect, it } from 'vitest'
// Real-SQLite integration test via node:sqlite (better-sqlite3 is compiled for the
// Electron ABI and cannot load under vitest's Node runtime). The migration SQL is
// plain SQLite, so semantics are identical.
import { DatabaseSync } from 'node:sqlite'
import { buildMigrations } from './migrations'
import { comparableWorkspacePath } from './workspacePaths'

function openMigratedThrough(id: number, windowsPaths: boolean): DatabaseSync {
  const db = new DatabaseSync(':memory:')
  db.exec('PRAGMA foreign_keys = ON')
  // Mirror Database's setup: the Windows migration SQL calls the app-registered
  // Unicode-aware key function instead of SQLite's ASCII-only LOWER()/NOCASE.
  db.function('canopy_path_key', { deterministic: true }, (path) =>
    comparableWorkspacePath(String(path), windowsPaths ? 'win32' : 'linux'),
  )
  for (const m of buildMigrations(windowsPaths)) {
    if (m.id <= id) db.exec(m.up)
  }
  return db
}

function applyMigration(db: DatabaseSync, id: number, windowsPaths: boolean): void {
  const migration = buildMigrations(windowsPaths).find((m) => m.id === id)
  if (!migration) throw new Error(`migration ${id} not found`)
  db.exec(migration.up)
}

/**
 * Reproduces the ghost-window disease observed in a real Windows profile: two
 * workspace rows for one directory (native-dialog backslash spelling vs git
 * forward-slash spelling), layouts and a skill stranded under the STALE row, and the
 * kept row holding layouts of its own so the merge has to resolve conflicts:
 * - the workspace root exists under BOTH rows → the kept row's NEWER copy must win
 * - `.../sub` exists under both rows with the STALE row's copy newer → it must win
 * - `.../wt` (stale) vs `.../WT` (keep) diverge only by case → must collapse to the
 *   newer copy on Windows
 */
function seedWindowsDisease(): DatabaseSync {
  const db = openMigratedThrough(10, true)
  const insertWs = db.prepare(
    'INSERT INTO workspaces (id, path, name, is_git_repo, last_opened) VALUES (?, ?, ?, 1, ?)',
  )
  insertWs.run('stale-id', 'C:\\source\\Repo', 'Repo', '2026-07-01 10:00:00')
  insertWs.run('keep-id', 'C:/source/Repo', 'Repo', '2026-07-20 10:00:00')

  const insertLayout = db.prepare(
    'INSERT INTO workspace_layouts (workspace_id, worktree_path, layout_json, updated_at) VALUES (?, ?, ?, ?)',
  )
  insertLayout.run('stale-id', 'C:\\source\\Repo', '{"panes":["old"]}', '2026-07-01 10:00:00')
  insertLayout.run('stale-id', 'C:/source/Repo', '{"panes":["new"]}', '2026-07-10 10:00:00')
  insertLayout.run('stale-id', 'C:/source/Repo/wt', '{"panes":["wt"]}', '2026-07-05 10:00:00')
  insertLayout.run(
    'stale-id',
    'C:/source/Repo/sub',
    '{"panes":["stale-newer"]}',
    '2026-07-09 10:00:00',
  )
  insertLayout.run('keep-id', 'C:/source/Repo', '{"panes":["keep-newer"]}', '2026-07-22 10:00:00')
  insertLayout.run('keep-id', 'C:/source/Repo/sub', '{"panes":["keep-old"]}', '2026-07-02 10:00:00')
  insertLayout.run('keep-id', 'C:/source/Repo/WT', '{"panes":["wt-upper"]}', '2026-07-06 10:00:00')

  db.prepare(
    "INSERT INTO skill_definitions (id, name, prompt, source_type, source_uri, workspace_id) VALUES ('sk1', 'skill', 'p', 'local', 'uri', ?)",
  ).run('stale-id')

  return db
}

describe('migration 11 on Windows (windowsPaths: true)', () => {
  it('merges separator-divergent duplicate workspaces into the most recently opened row', () => {
    const db = seedWindowsDisease()
    applyMigration(db, 11, true)

    const rows = db.prepare('SELECT id, path FROM workspaces ORDER BY id').all() as {
      id: string
      path: string
    }[]
    expect(rows).toEqual([{ id: 'keep-id', path: 'C:/source/Repo' }])
  })

  it('repoints stranded layouts to the kept row, resolving conflicts to the newest', () => {
    const db = seedWindowsDisease()
    applyMigration(db, 11, true)

    const layouts = db
      .prepare(
        'SELECT workspace_id, worktree_path, layout_json FROM workspace_layouts ORDER BY worktree_path',
      )
      .all() as { workspace_id: string; worktree_path: string; layout_json: string }[]
    expect(layouts).toEqual([
      // Workspace root exists under both rows — the kept row's NEWER copy wins.
      {
        workspace_id: 'keep-id',
        worktree_path: 'C:/source/Repo',
        layout_json: '{"panes":["keep-newer"]}',
      },
      // Case-divergent worktree pair collapses to the newer copy.
      {
        workspace_id: 'keep-id',
        worktree_path: 'C:/source/Repo/WT',
        layout_json: '{"panes":["wt-upper"]}',
      },
      // Same-spelling conflict where the STALE row's copy is newer — it must win.
      {
        workspace_id: 'keep-id',
        worktree_path: 'C:/source/Repo/sub',
        layout_json: '{"panes":["stale-newer"]}',
      },
    ])
  })

  it('breaks equal-timestamp layout conflicts by rowid (later write wins)', () => {
    // LayoutStore stamps datetime('now') with one-second precision, so equal
    // timestamps are routine. The higher rowid is the later write and must win —
    // regardless of which workspace row it sat under.
    const db = openMigratedThrough(10, true)
    const insertWs = db.prepare(
      'INSERT INTO workspaces (id, path, name, is_git_repo, last_opened) VALUES (?, ?, ?, 1, ?)',
    )
    insertWs.run('keep-id', 'C:/source/Repo', 'Repo', '2026-07-20 10:00:00')
    insertWs.run('stale-id', 'C:\\source\\Repo', 'Repo', '2026-07-01 10:00:00')

    const insertLayout = db.prepare(
      'INSERT INTO workspace_layouts (workspace_id, worktree_path, layout_json, updated_at) VALUES (?, ?, ?, ?)',
    )
    // rowid 1 under the KEPT row, rowid 2 under the stale row — same timestamp.
    insertLayout.run(
      'keep-id',
      'C:/source/Repo',
      '{"panes":["first-write"]}',
      '2026-07-15 10:00:00',
    )
    insertLayout.run(
      'stale-id',
      'C:/source/Repo',
      '{"panes":["later-write"]}',
      '2026-07-15 10:00:00',
    )
    applyMigration(db, 11, true)

    const layouts = db.prepare('SELECT workspace_id, layout_json FROM workspace_layouts').all() as {
      workspace_id: string
      layout_json: string
    }[]
    expect(layouts).toEqual([{ workspace_id: 'keep-id', layout_json: '{"panes":["later-write"]}' }])
  })

  it('repoints skill_definitions instead of losing them to the FK cascade', () => {
    const db = seedWindowsDisease()
    applyMigration(db, 11, true)

    const skill = db
      .prepare('SELECT workspace_id FROM skill_definitions WHERE id = ?')
      .get('sk1') as {
      workspace_id: string
    }
    expect(skill.workspace_id).toBe('keep-id')
  })

  it('merges case-divergent duplicates, preserving the kept row casing', () => {
    const db = openMigratedThrough(10, true)
    const insertWs = db.prepare(
      'INSERT INTO workspaces (id, path, name, is_git_repo, last_opened) VALUES (?, ?, ?, 1, ?)',
    )
    insertWs.run('lower-id', 'c:/source/repo', 'repo', '2026-07-01 10:00:00')
    insertWs.run('upper-id', 'C:/Source/Repo', 'Repo', '2026-07-20 10:00:00')
    applyMigration(db, 11, true)

    const rows = db.prepare('SELECT id, path FROM workspaces').all() as {
      id: string
      path: string
    }[]
    expect(rows).toEqual([{ id: 'upper-id', path: 'C:/Source/Repo' }])
  })

  it('merges non-ASCII case-divergent duplicates (SQLite LOWER/NOCASE would miss them)', () => {
    // canopy_path_key delegates to JS Unicode folding, so Ł ≡ ł — the built-in
    // ASCII-only LOWER()/NOCASE would treat these as two different directories.
    const db = openMigratedThrough(10, true)
    const insertWs = db.prepare(
      'INSERT INTO workspaces (id, path, name, is_git_repo, last_opened) VALUES (?, ?, ?, 1, ?)',
    )
    insertWs.run('old-id', 'c:/users/łukasz/repo', 'repo', '2026-07-01 10:00:00')
    insertWs.run('new-id', 'C:/Users/Łukasz/Repo', 'Repo', '2026-07-20 10:00:00')

    const insertLayout = db.prepare(
      'INSERT INTO workspace_layouts (workspace_id, worktree_path, layout_json, updated_at) VALUES (?, ?, ?, ?)',
    )
    insertLayout.run('old-id', 'c:/users/łukasz/repo', '{"panes":["old"]}', '2026-07-01 10:00:00')
    insertLayout.run('new-id', 'C:/Users/Łukasz/Repo', '{"panes":["new"]}', '2026-07-20 10:00:00')
    applyMigration(db, 11, true)

    const rows = db.prepare('SELECT id, path FROM workspaces').all() as {
      id: string
      path: string
    }[]
    expect(rows).toEqual([{ id: 'new-id', path: 'C:/Users/Łukasz/Repo' }])
    const layouts = db
      .prepare('SELECT workspace_id, worktree_path, layout_json FROM workspace_layouts')
      .all() as { workspace_id: string; worktree_path: string; layout_json: string }[]
    expect(layouts).toEqual([
      {
        workspace_id: 'new-id',
        worktree_path: 'C:/Users/Łukasz/Repo',
        layout_json: '{"panes":["new"]}',
      },
    ])
  })

  it('is idempotent', () => {
    const db = seedWindowsDisease()
    applyMigration(db, 11, true)
    const before = db.prepare('SELECT COUNT(*) AS n FROM workspace_layouts').get() as { n: number }
    applyMigration(db, 11, true)
    const after = db.prepare('SELECT COUNT(*) AS n FROM workspace_layouts').get() as { n: number }
    expect(after.n).toBe(before.n)
    expect((db.prepare('SELECT COUNT(*) AS n FROM workspaces').get() as { n: number }).n).toBe(1)
  })
})

describe('migration 11 on POSIX (windowsPaths: false)', () => {
  it('treats a literal backslash as a filename character, not a separator', () => {
    // `/tmp/repo\name` and `/tmp/repo/name` are two DIFFERENT directories on POSIX.
    const db = openMigratedThrough(10, false)
    const insertWs = db.prepare(
      'INSERT INTO workspaces (id, path, name, is_git_repo, last_opened) VALUES (?, ?, ?, 1, ?)',
    )
    insertWs.run('bs-id', '/tmp/repo\\name', 'repo', '2026-07-01 10:00:00')
    insertWs.run('fs-id', '/tmp/repo/name', 'repo', '2026-07-20 10:00:00')

    const insertLayout = db.prepare(
      'INSERT INTO workspace_layouts (workspace_id, worktree_path, layout_json, updated_at) VALUES (?, ?, ?, ?)',
    )
    insertLayout.run('bs-id', '/tmp/repo\\name', '{"panes":["bs"]}', '2026-07-01 10:00:00')
    insertLayout.run('fs-id', '/tmp/repo/name', '{"panes":["fs"]}', '2026-07-20 10:00:00')
    applyMigration(db, 11, false)

    const workspaces = db.prepare('SELECT id, path FROM workspaces ORDER BY id').all() as {
      id: string
      path: string
    }[]
    // Both survive, neither path is rewritten.
    expect(workspaces).toEqual([
      { id: 'bs-id', path: '/tmp/repo\\name' },
      { id: 'fs-id', path: '/tmp/repo/name' },
    ])
    const layouts = db
      .prepare('SELECT workspace_id, worktree_path FROM workspace_layouts ORDER BY workspace_id')
      .all() as { workspace_id: string; worktree_path: string }[]
    expect(layouts).toEqual([
      { workspace_id: 'bs-id', worktree_path: '/tmp/repo\\name' },
      { workspace_id: 'fs-id', worktree_path: '/tmp/repo/name' },
    ])
  })

  it('keeps case-divergent directories separate', () => {
    const db = openMigratedThrough(10, false)
    const insertWs = db.prepare(
      'INSERT INTO workspaces (id, path, name, is_git_repo, last_opened) VALUES (?, ?, ?, 1, ?)',
    )
    insertWs.run('a', '/home/user/repo', 'repo', '2026-07-01 10:00:00')
    insertWs.run('b', '/home/user/Repo', 'Repo', '2026-07-20 10:00:00')
    applyMigration(db, 11, false)

    const rows = db.prepare('SELECT id FROM workspaces ORDER BY id').all() as { id: string }[]
    expect(rows).toEqual([{ id: 'a' }, { id: 'b' }])
  })

  it('still merges exact-duplicate layouts deterministically', () => {
    // Exact duplicates cannot exist in `workspaces` (path is UNIQUE), but layouts
    // pointing at one workspace can still carry same-path rows only when timestamps
    // collide across workspace merges — on POSIX nothing merges, so layouts stay.
    const db = openMigratedThrough(10, false)
    db.prepare(
      'INSERT INTO workspaces (id, path, name, is_git_repo, last_opened) VALUES (?, ?, ?, 1, ?)',
    ).run('a', '/home/user/repo', 'repo', '2026-07-01 10:00:00')
    db.prepare(
      'INSERT INTO workspace_layouts (workspace_id, worktree_path, layout_json, updated_at) VALUES (?, ?, ?, ?)',
    ).run('a', '/home/user/repo', '{"panes":[]}', '2026-07-01 10:00:00')
    applyMigration(db, 11, false)

    const layouts = db.prepare('SELECT workspace_id, worktree_path FROM workspace_layouts').all()
    expect(layouts).toEqual([{ workspace_id: 'a', worktree_path: '/home/user/repo' }])
  })
})
