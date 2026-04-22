import { randomUUID } from 'crypto'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'
import type { MessageRole, SdkMessageRecord, SdkMessageRow } from './sdkAgentRows'
import { sdkMessageFromRow } from './sdkAgentRows'
import type { ContentBlock, ConversationId, MessageId } from '../sdkAgents/types'
import { asMessageId } from '../sdkAgents/types'

export interface AppendMessageInput {
  conversationId: ConversationId
  parentSubagentId?: string | null
  role: MessageRole
  content: string
  contentBlocks: ContentBlock[]
  thinking?: string | null
  toolCalls?: unknown
  tokensIn?: number | null
  tokensOut?: number | null
  model?: string | null
  /** Optional caller-provided id. Generated otherwise. */
  id?: MessageId
}

export class SdkMessageStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  append(input: AppendMessageInput): SdkMessageRecord {
    const id = input.id ?? asMessageId(randomUUID())
    // UPSERT: the Claude Agent SDK can yield multiple `type: 'assistant'`
    // messages with the same `message.id` (streaming updates, session resume
    // replays). Idempotent write — last payload wins, matching the SDK's
    // "message as current cumulative state" semantics.
    this.db
      .prepare(
        `INSERT INTO sdk_messages (
           id, conversation_id, parent_subagent_id, role, content, thinking, content_json,
           tool_calls_json, tokens_in, tokens_out, model
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           parent_subagent_id = COALESCE(excluded.parent_subagent_id, sdk_messages.parent_subagent_id),
           content = excluded.content,
           thinking = CASE
             WHEN excluded.thinking <> '' OR sdk_messages.thinking = '' THEN excluded.thinking
             ELSE sdk_messages.thinking
           END,
           content_json = excluded.content_json,
           tool_calls_json = excluded.tool_calls_json,
           tokens_in = COALESCE(excluded.tokens_in, sdk_messages.tokens_in),
           tokens_out = COALESCE(excluded.tokens_out, sdk_messages.tokens_out),
           model = COALESCE(excluded.model, sdk_messages.model)`,
      )
      .run(
        id,
        input.conversationId,
        input.parentSubagentId ?? null,
        input.role,
        input.content,
        input.thinking ?? '',
        JSON.stringify(input.contentBlocks),
        input.toolCalls !== undefined ? JSON.stringify(input.toolCalls) : null,
        input.tokensIn ?? null,
        input.tokensOut ?? null,
        input.model ?? null,
      )
    return this.getById(id)!
  }

  getById(id: MessageId): SdkMessageRecord | undefined {
    const row = this.db.prepare('SELECT * FROM sdk_messages WHERE id = ?').get(id) as
      | SdkMessageRow
      | undefined
    return row ? sdkMessageFromRow(row) : undefined
  }

  listByConversation(conversationId: ConversationId): SdkMessageRecord[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM sdk_messages
         WHERE conversation_id = ?
         ORDER BY created_at ASC, rowid ASC`,
      )
      .all(conversationId) as SdkMessageRow[]
    return rows.map(sdkMessageFromRow)
  }

  getLatest(conversationId: ConversationId): SdkMessageRecord | undefined {
    const row = this.db
      .prepare(
        `SELECT * FROM sdk_messages
         WHERE conversation_id = ?
         ORDER BY created_at DESC, rowid DESC
         LIMIT 1`,
      )
      .get(conversationId) as SdkMessageRow | undefined
    return row ? sdkMessageFromRow(row) : undefined
  }

  updateTokenUsage(
    id: MessageId,
    usage: { tokensIn?: number | null; tokensOut?: number | null },
  ): void {
    this.db
      .prepare(
        `UPDATE sdk_messages
         SET tokens_in = COALESCE(?, tokens_in),
             tokens_out = COALESCE(?, tokens_out)
         WHERE id = ?`,
      )
      .run(usage.tokensIn ?? null, usage.tokensOut ?? null, id)
  }

  updateContent(id: MessageId, content: { content: string; contentBlocks: ContentBlock[] }): void {
    this.db
      .prepare(
        `UPDATE sdk_messages
         SET content = ?, content_json = ?
         WHERE id = ?`,
      )
      .run(content.content, JSON.stringify(content.contentBlocks), id)
  }

  appendThinking(id: MessageId, delta: string): void {
    this.db
      .prepare(
        `UPDATE sdk_messages
         SET thinking = thinking || ?
         WHERE id = ?`,
      )
      .run(delta, id)
  }
}
