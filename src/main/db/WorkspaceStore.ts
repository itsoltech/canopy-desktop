import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'
import type { WorkspaceRow } from './types'
import { randomUUID } from 'crypto'

export class WorkspaceStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  list(limit = 10): WorkspaceRow[] {
    return this.db
      .prepare('SELECT * FROM workspaces ORDER BY last_opened DESC LIMIT ?')
      .all(limit) as WorkspaceRow[]
  }

  get(id: string): WorkspaceRow | undefined {
    return this.db.prepare('SELECT * FROM workspaces WHERE id = ?').get(id) as
      | WorkspaceRow
      | undefined
  }

  getByPath(path: string): WorkspaceRow | undefined {
    return this.db.prepare('SELECT * FROM workspaces WHERE path = ?').get(path) as
      | WorkspaceRow
      | undefined
  }

  upsert(workspace: { path: string; name: string; isGitRepo: boolean }): WorkspaceRow {
    const existing = this.getByPath(workspace.path)

    if (existing) {
      this.db
        .prepare(
          `UPDATE workspaces
           SET name = ?, is_git_repo = ?, last_opened = datetime('now')
           WHERE id = ?`,
        )
        .run(workspace.name, workspace.isGitRepo ? 1 : 0, existing.id)
      return this.get(existing.id)!
    }

    const id = randomUUID()
    this.db
      .prepare(
        `INSERT INTO workspaces (id, path, name, is_git_repo, last_opened)
         VALUES (?, ?, ?, ?, datetime('now'))`,
      )
      .run(id, workspace.path, workspace.name, workspace.isGitRepo ? 1 : 0)
    return this.get(id)!
  }

  updateGitCache(
    id: string,
    cache: {
      branch?: string | null
      dirty?: boolean | null
      aheadBehind?: string | null
      worktreeCount?: number | null
    },
  ): void {
    this.db
      .prepare(
        `UPDATE workspaces
         SET cached_branch = ?, cached_dirty = ?, cached_ahead_behind = ?, cached_worktree_count = ?
         WHERE id = ?`,
      )
      .run(
        cache.branch ?? null,
        cache.dirty === undefined || cache.dirty === null ? null : cache.dirty ? 1 : 0,
        cache.aheadBehind ?? null,
        cache.worktreeCount ?? null,
        id,
      )
  }

  touch(id: string): void {
    this.db.prepare("UPDATE workspaces SET last_opened = datetime('now') WHERE id = ?").run(id)
  }

  remove(id: string): void {
    this.db.prepare('DELETE FROM workspaces WHERE id = ?').run(id)
  }
}
