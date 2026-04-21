import { randomUUID } from 'crypto'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'
import type { SdkToolEventRecord, SdkToolEventRow } from './sdkAgentRows'
import { sdkToolEventFromRow } from './sdkAgentRows'
import type { ConversationId, MessageId, ToolDecision } from '../sdkAgents/types'

export interface StartToolEventInput {
  messageId: MessageId
  conversationId: ConversationId
  toolName: string
  input: Record<string, unknown>
  decision?: ToolDecision | 'auto' | null
  /** Optional caller-provided id; random UUID otherwise. */
  id?: string
}

export interface CompleteToolEventInput {
  id: string
  resultText: string
  isError: boolean
  durationMs: number
}

export class SdkToolEventStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  start(input: StartToolEventInput): SdkToolEventRecord {
    const id = input.id ?? randomUUID()
    // UPSERT: tool_use ids can re-appear when the SDK replays assistant
    // messages on resume. Idempotent write — don't overwrite a completed
    // result with a fresh start row.
    this.db
      .prepare(
        `INSERT INTO sdk_tool_events (
           id, message_id, conversation_id, tool_name,
           input_json, decision
         ) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           message_id = excluded.message_id,
           tool_name = excluded.tool_name,
           input_json = excluded.input_json,
           decision = COALESCE(excluded.decision, sdk_tool_events.decision)`,
      )
      .run(
        id,
        input.messageId,
        input.conversationId,
        input.toolName,
        JSON.stringify(input.input),
        input.decision ?? null,
      )
    return this.getById(id)!
  }

  complete(input: CompleteToolEventInput): void {
    this.db
      .prepare(
        `UPDATE sdk_tool_events
         SET result_text = ?, is_error = ?, duration_ms = ?
         WHERE id = ?`,
      )
      .run(input.resultText, input.isError ? 1 : 0, input.durationMs, input.id)
  }

  getById(id: string): SdkToolEventRecord | undefined {
    const row = this.db.prepare('SELECT * FROM sdk_tool_events WHERE id = ?').get(id) as
      | SdkToolEventRow
      | undefined
    return row ? sdkToolEventFromRow(row) : undefined
  }

  listByMessage(messageId: MessageId): SdkToolEventRecord[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM sdk_tool_events
         WHERE message_id = ?
         ORDER BY created_at ASC, rowid ASC`,
      )
      .all(messageId) as SdkToolEventRow[]
    return rows.map(sdkToolEventFromRow)
  }

  listByConversation(conversationId: ConversationId): SdkToolEventRecord[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM sdk_tool_events
         WHERE conversation_id = ?
         ORDER BY created_at ASC, rowid ASC`,
      )
      .all(conversationId) as SdkToolEventRow[]
    return rows.map(sdkToolEventFromRow)
  }
}
