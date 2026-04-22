import type {
  AskUserQuestionAnswer,
  ConversationId,
  ContentBlock,
  EffortLevel,
  MessageId,
  PermissionMode,
  SdkSessionId,
  ToolDecision,
} from '../sdkAgents/types'
import {
  asConversationId,
  asMessageId,
  asSdkSessionId,
  normalizeEffortLevel,
  normalizePermissionMode,
} from '../sdkAgents/types'

// SQLite's datetime('now') emits "YYYY-MM-DD HH:MM:SS" (UTC, no T, no Z),
// which JS parses as local time. Normalize to strict ISO-8601 UTC so
// the renderer's Timestamp atom computes correct relative times.
function toIsoUtc(s: string): string {
  if (!s) return s
  if (s.endsWith('Z') || /[+-]\d{2}:?\d{2}$/.test(s)) return s
  const iso = s.includes('T') ? s : s.replace(' ', 'T')
  return iso + 'Z'
}

// --- Raw row shapes (1:1 with SQLite columns) ---

export interface ConversationRow {
  id: string
  workspace_id: string
  worktree_path: string
  agent_profile_id: string
  sdk_session_id: string | null
  title: string | null
  model: string
  permission_mode: string
  effort_level: string | null
  status: string
  created_at: string
  updated_at: string
}

export interface SdkMessageRow {
  id: string
  conversation_id: string
  parent_subagent_id: string | null
  role: string
  content: string
  thinking: string
  content_json: string
  tool_calls_json: string | null
  tokens_in: number | null
  tokens_out: number | null
  model: string | null
  created_at: string
}

export interface SdkToolEventRow {
  id: string
  message_id: string
  conversation_id: string
  parent_subagent_id: string | null
  tool_name: string
  input_json: string
  result_text: string | null
  is_error: number
  decision: string | null
  duration_ms: number | null
  created_at: string
  answers_json: string | null
}

export interface SdkAttachmentRow {
  id: string
  message_id: string
  conversation_id: string
  kind: string
  filename: string
  path: string
  mime_type: string
  size_bytes: number
  created_at: string
}

// --- Domain types (converted from rows) ---

export type ConversationStatus = 'active' | 'ended' | 'error' | 'cancelled'
export type MessageRole = 'user' | 'assistant' | 'system' | 'tool'

export interface Conversation {
  id: ConversationId
  workspaceId: string
  worktreePath: string
  agentProfileId: string
  sdkSessionId: SdkSessionId | null
  title: string | null
  model: string
  permissionMode: PermissionMode
  effortLevel: EffortLevel | null
  status: ConversationStatus
  createdAt: string
  updatedAt: string
}

export interface SdkMessageRecord {
  id: MessageId
  conversationId: ConversationId
  parentSubagentId: string | null
  role: MessageRole
  content: string
  thinking: string
  contentBlocks: ContentBlock[]
  toolCalls?: unknown
  tokensIn: number | null
  tokensOut: number | null
  /** The model that actually produced this message, when known. */
  model: string | null
  createdAt: string
}

export interface SdkToolEventRecord {
  id: string
  messageId: MessageId
  conversationId: ConversationId
  parentSubagentId: string | null
  toolName: string
  input: Record<string, unknown>
  resultText: string | null
  isError: boolean
  decision: ToolDecision | 'auto' | null
  durationMs: number | null
  createdAt: string
  answers?: Record<string, AskUserQuestionAnswer>
}

export type SdkAttachmentKind = 'image' | 'text'

export interface SdkAttachmentRecord {
  id: string
  messageId: MessageId
  conversationId: ConversationId
  kind: SdkAttachmentKind
  filename: string
  path: string
  mimeType: string
  sizeBytes: number
  createdAt: string
}

// --- Row → domain converters ---

export function conversationFromRow(row: ConversationRow): Conversation {
  return {
    id: asConversationId(row.id),
    workspaceId: row.workspace_id,
    worktreePath: row.worktree_path,
    agentProfileId: row.agent_profile_id,
    sdkSessionId: row.sdk_session_id ? asSdkSessionId(row.sdk_session_id) : null,
    title: row.title,
    model: row.model,
    permissionMode: normalizePermissionMode(row.permission_mode),
    effortLevel: normalizeEffortLevel(row.effort_level),
    status: row.status as ConversationStatus,
    createdAt: toIsoUtc(row.created_at),
    updatedAt: toIsoUtc(row.updated_at),
  }
}

export function sdkMessageFromRow(row: SdkMessageRow): SdkMessageRecord {
  return {
    id: asMessageId(row.id),
    conversationId: asConversationId(row.conversation_id),
    parentSubagentId: row.parent_subagent_id,
    role: row.role as MessageRole,
    content: row.content,
    thinking: row.thinking,
    contentBlocks: JSON.parse(row.content_json) as ContentBlock[],
    toolCalls: row.tool_calls_json ? (JSON.parse(row.tool_calls_json) as unknown) : undefined,
    tokensIn: row.tokens_in,
    tokensOut: row.tokens_out,
    model: row.model,
    createdAt: toIsoUtc(row.created_at),
  }
}

export function sdkToolEventFromRow(row: SdkToolEventRow): SdkToolEventRecord {
  return {
    id: row.id,
    messageId: asMessageId(row.message_id),
    conversationId: asConversationId(row.conversation_id),
    parentSubagentId: row.parent_subagent_id,
    toolName: row.tool_name,
    input: JSON.parse(row.input_json) as Record<string, unknown>,
    resultText: row.result_text,
    isError: row.is_error === 1,
    decision: row.decision as ToolDecision | 'auto' | null,
    durationMs: row.duration_ms,
    createdAt: toIsoUtc(row.created_at),
    ...(row.answers_json
      ? { answers: JSON.parse(row.answers_json) as Record<string, AskUserQuestionAnswer> }
      : {}),
  }
}

export function sdkAttachmentFromRow(row: SdkAttachmentRow): SdkAttachmentRecord {
  return {
    id: row.id,
    messageId: asMessageId(row.message_id),
    conversationId: asConversationId(row.conversation_id),
    kind: row.kind as SdkAttachmentKind,
    filename: row.filename,
    path: row.path,
    mimeType: row.mime_type,
    sizeBytes: row.size_bytes,
    createdAt: toIsoUtc(row.created_at),
  }
}
