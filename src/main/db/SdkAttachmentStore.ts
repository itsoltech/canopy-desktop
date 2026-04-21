import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'
import type { SdkAttachmentKind, SdkAttachmentRecord, SdkAttachmentRow } from './sdkAgentRows'
import { sdkAttachmentFromRow } from './sdkAgentRows'
import type { ConversationId, MessageId } from '../sdkAgents/types'

export interface InsertAttachmentInput {
  id: string
  messageId: MessageId
  conversationId: ConversationId
  kind: SdkAttachmentKind
  filename: string
  path: string
  mimeType: string
  sizeBytes: number
}

/**
 * Metadata-only store. Attachment blobs live on disk under
 * `app.getPath('userData')/attachments/<conversationId>/<id>.<ext>`; this
 * store owns the DB row that points at them. Phase 9 adds the
 * `attachmentPipeline.ts` helper that writes the bytes.
 */
export class SdkAttachmentStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  insert(input: InsertAttachmentInput): SdkAttachmentRecord {
    this.db
      .prepare(
        `INSERT INTO sdk_attachments (
           id, message_id, conversation_id, kind,
           filename, path, mime_type, size_bytes
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        input.id,
        input.messageId,
        input.conversationId,
        input.kind,
        input.filename,
        input.path,
        input.mimeType,
        input.sizeBytes,
      )
    return this.getById(input.id)!
  }

  getById(id: string): SdkAttachmentRecord | undefined {
    const row = this.db.prepare('SELECT * FROM sdk_attachments WHERE id = ?').get(id) as
      | SdkAttachmentRow
      | undefined
    return row ? sdkAttachmentFromRow(row) : undefined
  }

  listByMessage(messageId: MessageId): SdkAttachmentRecord[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM sdk_attachments
         WHERE message_id = ?
         ORDER BY created_at ASC, rowid ASC`,
      )
      .all(messageId) as SdkAttachmentRow[]
    return rows.map(sdkAttachmentFromRow)
  }

  listByConversation(conversationId: ConversationId): SdkAttachmentRecord[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM sdk_attachments
         WHERE conversation_id = ?
         ORDER BY created_at ASC, rowid ASC`,
      )
      .all(conversationId) as SdkAttachmentRow[]
    return rows.map(sdkAttachmentFromRow)
  }

  deleteById(id: string): void {
    this.db.prepare('DELETE FROM sdk_attachments WHERE id = ?').run(id)
  }
}
