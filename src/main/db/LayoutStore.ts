import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'

export class LayoutStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  save(workspaceId: string, worktreePath: string, layoutJson: string): void {
    this.db
      .prepare(
        `INSERT OR REPLACE INTO workspace_layouts (workspace_id, worktree_path, layout_json, updated_at)
         VALUES (?, ?, ?, datetime('now'))`,
      )
      .run(workspaceId, worktreePath, layoutJson)
  }

  get(workspaceId: string, worktreePath: string): string | null {
    const row = this.db
      .prepare(
        'SELECT layout_json FROM workspace_layouts WHERE workspace_id = ? AND worktree_path = ?',
      )
      .get(workspaceId, worktreePath) as { layout_json: string } | undefined
    return row?.layout_json ?? null
  }

  getAll(workspaceId: string): { worktree_path: string; layout_json: string }[] {
    return this.db
      .prepare('SELECT worktree_path, layout_json FROM workspace_layouts WHERE workspace_id = ?')
      .all(workspaceId) as { worktree_path: string; layout_json: string }[]
  }

  delete(workspaceId: string, worktreePath: string): void {
    this.db
      .prepare('DELETE FROM workspace_layouts WHERE workspace_id = ? AND worktree_path = ?')
      .run(workspaceId, worktreePath)
  }

  deleteAll(workspaceId: string): void {
    this.db.prepare('DELETE FROM workspace_layouts WHERE workspace_id = ?').run(workspaceId)
  }
}
