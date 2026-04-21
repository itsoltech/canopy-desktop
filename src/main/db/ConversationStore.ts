import { randomUUID } from 'crypto'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'
import type { Conversation, ConversationRow, ConversationStatus } from './sdkAgentRows'
import { conversationFromRow } from './sdkAgentRows'
import type { ConversationId, PermissionMode, SdkSessionId } from '../sdkAgents/types'
import { asConversationId } from '../sdkAgents/types'

export interface CreateConversationInput {
  workspaceId: string
  worktreePath: string
  agentProfileId: string
  model: string
  permissionMode: PermissionMode
  title?: string | null
}

export interface ConversationSearchHit {
  conversationId: ConversationId
  title: string | null
  snippet: string
  createdAt: string
}

export class ConversationStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  create(input: CreateConversationInput): Conversation {
    const id = randomUUID()
    this.db
      .prepare(
        `INSERT INTO conversations (
           id, workspace_id, worktree_path, agent_profile_id,
           sdk_session_id, title, model, permission_mode, status
         ) VALUES (?, ?, ?, ?, NULL, ?, ?, ?, 'active')`,
      )
      .run(
        id,
        input.workspaceId,
        input.worktreePath,
        input.agentProfileId,
        input.title ?? null,
        input.model,
        input.permissionMode,
      )
    return this.get(asConversationId(id))!
  }

  get(id: ConversationId): Conversation | undefined {
    const row = this.db.prepare('SELECT * FROM conversations WHERE id = ?').get(id) as
      | ConversationRow
      | undefined
    return row ? conversationFromRow(row) : undefined
  }

  listByWorkspace(workspaceId: string, limit = 100): Conversation[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM conversations
         WHERE workspace_id = ?
         ORDER BY updated_at DESC
         LIMIT ?`,
      )
      .all(workspaceId, limit) as ConversationRow[]
    return rows.map(conversationFromRow)
  }

  listByWorktree(workspaceId: string, worktreePath: string, limit = 100): Conversation[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM conversations
         WHERE workspace_id = ? AND worktree_path = ?
         ORDER BY updated_at DESC
         LIMIT ?`,
      )
      .all(workspaceId, worktreePath, limit) as ConversationRow[]
    return rows.map(conversationFromRow)
  }

  listIdsByWorktree(workspaceId: string, worktreePath: string): ConversationId[] {
    const rows = this.db
      .prepare(
        `SELECT id FROM conversations
         WHERE workspace_id = ? AND worktree_path = ?`,
      )
      .all(workspaceId, worktreePath) as { id: string }[]
    return rows.map((r) => asConversationId(r.id))
  }

  listIdsByWorktreePath(worktreePath: string): ConversationId[] {
    const rows = this.db
      .prepare(`SELECT id FROM conversations WHERE worktree_path = ?`)
      .all(worktreePath) as { id: string }[]
    return rows.map((r) => asConversationId(r.id))
  }

  rename(id: ConversationId, title: string | null): void {
    this.db
      .prepare(
        `UPDATE conversations
         SET title = ?, updated_at = datetime('now')
         WHERE id = ?`,
      )
      .run(title, id)
  }

  updateStatus(id: ConversationId, status: ConversationStatus): void {
    this.db
      .prepare(
        `UPDATE conversations
         SET status = ?, updated_at = datetime('now')
         WHERE id = ?`,
      )
      .run(status, id)
  }

  setSdkSessionId(id: ConversationId, sdkSessionId: SdkSessionId): void {
    this.db
      .prepare(
        `UPDATE conversations
         SET sdk_session_id = ?, updated_at = datetime('now')
         WHERE id = ?`,
      )
      .run(sdkSessionId, id)
  }

  setModel(id: ConversationId, model: string): void {
    this.db
      .prepare(
        `UPDATE conversations
         SET model = ?, updated_at = datetime('now')
         WHERE id = ?`,
      )
      .run(model, id)
  }

  setPermissionMode(id: ConversationId, mode: PermissionMode): void {
    this.db
      .prepare(
        `UPDATE conversations
         SET permission_mode = ?, updated_at = datetime('now')
         WHERE id = ?`,
      )
      .run(mode, id)
  }

  touch(id: ConversationId): void {
    this.db.prepare("UPDATE conversations SET updated_at = datetime('now') WHERE id = ?").run(id)
  }

  hardDelete(id: ConversationId): void {
    // FK cascade removes sdk_messages, sdk_tool_events, sdk_attachments.
    this.db.prepare('DELETE FROM conversations WHERE id = ?').run(id)
  }

  /**
   * FTS5-backed search over message content. Returns one hit per matched
   * message with a highlight snippet. Caller is responsible for quoting /
   * escaping raw user input before passing it as an FTS5 query.
   */
  search(workspaceId: string, query: string, limit = 50): ConversationSearchHit[] {
    const rows = this.db
      .prepare(
        `SELECT
           c.id                         AS conversation_id,
           c.title                      AS title,
           snippet(sdk_messages_fts, 0, '<mark>', '</mark>', '…', 16) AS snippet,
           c.created_at                 AS created_at
         FROM sdk_messages_fts
         JOIN sdk_messages m ON m.rowid = sdk_messages_fts.rowid
         JOIN conversations c ON c.id = m.conversation_id
         WHERE c.workspace_id = ? AND sdk_messages_fts MATCH ?
         ORDER BY rank
         LIMIT ?`,
      )
      .all(workspaceId, query, limit) as {
      conversation_id: string
      title: string | null
      snippet: string
      created_at: string
    }[]
    return rows.map((r) => ({
      conversationId: asConversationId(r.conversation_id),
      title: r.title,
      snippet: r.snippet,
      createdAt: r.created_at,
    }))
  }
}
