import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'
import { normalizeWorkspacePath } from './workspacePaths'

// Case-insensitive worktree-path matching on Windows (same rationale as the
// workspace lookup in WorkspaceStore). The PK itself stays BINARY, so save() first
// removes any case-divergent row to keep one layout per directory.
const WT_EQ =
  process.platform === 'win32' ? 'worktree_path = ? COLLATE NOCASE' : 'worktree_path = ?'

export class LayoutStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  save(workspaceId: string, worktreePath: string, layoutJson: string): void {
    const path = normalizeWorkspacePath(worktreePath)
    const write = this.db.transaction(() => {
      this.db
        .prepare(`DELETE FROM workspace_layouts WHERE workspace_id = ? AND ${WT_EQ}`)
        .run(workspaceId, path)
      this.db
        .prepare(
          `INSERT INTO workspace_layouts (workspace_id, worktree_path, layout_json, updated_at)
           VALUES (?, ?, ?, datetime('now'))`,
        )
        .run(workspaceId, path, layoutJson)
    })
    write()
  }

  get(workspaceId: string, worktreePath: string): string | null {
    const row = this.db
      .prepare(`SELECT layout_json FROM workspace_layouts WHERE workspace_id = ? AND ${WT_EQ}`)
      .get(workspaceId, normalizeWorkspacePath(worktreePath)) as { layout_json: string } | undefined
    return row?.layout_json ?? null
  }

  getAll(workspaceId: string): { worktree_path: string; layout_json: string }[] {
    return this.db
      .prepare('SELECT worktree_path, layout_json FROM workspace_layouts WHERE workspace_id = ?')
      .all(workspaceId) as { worktree_path: string; layout_json: string }[]
  }

  delete(workspaceId: string, worktreePath: string): void {
    this.db
      .prepare(`DELETE FROM workspace_layouts WHERE workspace_id = ? AND ${WT_EQ}`)
      .run(workspaceId, normalizeWorkspacePath(worktreePath))
  }

  deleteAll(workspaceId: string): void {
    this.db.prepare('DELETE FROM workspace_layouts WHERE workspace_id = ?').run(workspaceId)
  }

  getDistinctWorkspaceIds(): string[] {
    return (
      this.db.prepare('SELECT DISTINCT workspace_id FROM workspace_layouts').all() as {
        workspace_id: string
      }[]
    ).map((r) => r.workspace_id)
  }

  isClosed(): boolean {
    return this.database.isClosed()
  }
}
