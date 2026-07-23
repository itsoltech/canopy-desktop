import { describe, expect, it } from 'vitest'
// Real-SQLite integration test via node:sqlite (better-sqlite3 is compiled for the
// Electron ABI and cannot load under vitest's Node runtime). The migration SQL is
// plain SQLite, so semantics are identical.
import { DatabaseSync } from 'node:sqlite'
import { buildMigrations } from './migrations'

function openMigratedThrough(id: number, caseInsensitivePaths: boolean): DatabaseSync {
  const db = new DatabaseSync(':memory:')
  db.exec('PRAGMA foreign_keys = ON')
  for (const m of buildMigrations(caseInsensitivePaths)) {
    if (m.id <= id) db.exec(m.up)
  }
  return db
}

function applyMigration(db: DatabaseSync, id: number, caseInsensitivePaths: boolean): void {
  const migration = buildMigrations(caseInsensitivePaths).find((m) => m.id === id)
  if (!migration) throw new Error(`migration ${id} not found`)
  db.exec(migration.up)
}

interface SeededDb {
  db: DatabaseSync
  keepId: string
  staleId: string
}

/**
 * Reproduces the ghost-window disease observed in a real profile: two workspace rows
 * for one directory (native-dialog backslash spelling vs git forward-slash spelling),
 * layouts and a skill stranded under the STALE row, and a duplicate layout for the
 * same worktree in both spellings. The kept row also holds layouts of its own so the
 * merge has to resolve conflicts, not just repoint:
 * - the workspace root exists under BOTH rows → ON CONFLICT timestamp tie-break
 *   (keep's copy is newer and must win)
 * - `.../sub` exists under both rows with the same spelling → ON CONFLICT again,
 *   this time the stale row's copy is newer and must win
 * - `.../wt` (stale) vs `.../WT` (keep) diverge only by case → BINARY PK lets both
 *   coexist after the repoint; the second dedupe pass must collapse them on
 *   case-insensitive filesystems and leave both on case-sensitive ones
 */
function seedDuplicateWorkspaces(caseInsensitivePaths: boolean): SeededDb {
  const db = openMigratedThrough(10, caseInsensitivePaths)
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

  return { db, keepId: 'keep-id', staleId: 'stale-id' }
}

describe.each([{ caseInsensitivePaths: false }, { caseInsensitivePaths: true }])(
  'migration 11 (caseInsensitivePaths: $caseInsensitivePaths)',
  ({ caseInsensitivePaths }) => {
    it('merges separator-divergent duplicate workspaces into the most recently opened row', () => {
      const { db, keepId } = seedDuplicateWorkspaces(caseInsensitivePaths)
      applyMigration(db, 11, caseInsensitivePaths)

      const rows = db.prepare('SELECT id, path FROM workspaces ORDER BY id').all() as {
        id: string
        path: string
      }[]
      expect(rows).toEqual([{ id: keepId, path: 'C:/source/Repo' }])
    })

    it('repoints stranded layouts to the kept row, resolving conflicts to the newest', () => {
      const { db, keepId } = seedDuplicateWorkspaces(caseInsensitivePaths)
      applyMigration(db, 11, caseInsensitivePaths)

      const layouts = db
        .prepare(
          'SELECT workspace_id, worktree_path, layout_json FROM workspace_layouts ORDER BY worktree_path',
        )
        .all() as { workspace_id: string; worktree_path: string; layout_json: string }[]
      const expected = [
        // Workspace root exists under both rows — ON CONFLICT keeps the kept row's
        // NEWER copy over the repointed stale one.
        {
          workspace_id: keepId,
          worktree_path: 'C:/source/Repo',
          layout_json: '{"panes":["keep-newer"]}',
        },
        // Case-divergent worktree: collapsed by the second dedupe pass on
        // case-insensitive filesystems, left alone on case-sensitive ones.
        {
          workspace_id: keepId,
          worktree_path: 'C:/source/Repo/WT',
          layout_json: '{"panes":["wt-upper"]}',
        },
        // Same-spelling conflict where the STALE row's copy is newer — ON CONFLICT
        // DO UPDATE must fire and replace the kept row's older copy.
        {
          workspace_id: keepId,
          worktree_path: 'C:/source/Repo/sub',
          layout_json: '{"panes":["stale-newer"]}',
        },
        ...(caseInsensitivePaths
          ? []
          : [
              {
                workspace_id: keepId,
                worktree_path: 'C:/source/Repo/wt',
                layout_json: '{"panes":["wt"]}',
              },
            ]),
      ]
      expect(layouts).toEqual(expected)
    })

    it('repoints skill_definitions instead of losing them to the FK cascade', () => {
      const { db, keepId } = seedDuplicateWorkspaces(caseInsensitivePaths)
      applyMigration(db, 11, caseInsensitivePaths)

      const skill = db
        .prepare('SELECT workspace_id FROM skill_definitions WHERE id = ?')
        .get('sk1') as {
        workspace_id: string
      }
      expect(skill.workspace_id).toBe(keepId)
    })

    it('is idempotent', () => {
      const { db } = seedDuplicateWorkspaces(caseInsensitivePaths)
      applyMigration(db, 11, caseInsensitivePaths)
      const before = db.prepare('SELECT COUNT(*) AS n FROM workspace_layouts').get() as {
        n: number
      }
      applyMigration(db, 11, caseInsensitivePaths)
      const after = db.prepare('SELECT COUNT(*) AS n FROM workspace_layouts').get() as { n: number }
      expect(after.n).toBe(before.n)
      expect((db.prepare('SELECT COUNT(*) AS n FROM workspaces').get() as { n: number }).n).toBe(1)
    })
  },
)

describe('migration 11 case handling', () => {
  it('merges case-divergent duplicates when the filesystem is case-insensitive', () => {
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
    // Kept row preserves its own (display) casing — nothing gets lowercased on disk.
    expect(rows).toEqual([{ id: 'upper-id', path: 'C:/Source/Repo' }])
  })

  it('keeps case-divergent directories separate on case-sensitive filesystems', () => {
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
})
