import { randomUUID } from 'crypto'
import { match, P } from 'ts-pattern'
import type {
  ContentBlock,
  ConversationId,
  PermissionMode,
  SdkAgentEvent,
  SdkSessionId,
} from './types'
import { asMessageId, asSdkSessionId } from './types'

/**
 * Loose SDK-message shape — typed by surface only, not re-exported from the
 * SDK types directly. The SDK's runtime shape is stable enough that a structural
 * match gets us the fields we need without pinning a deep type dependency here.
 */
export interface SdkLikeMessage {
  type: string
  session_id?: string
  uuid?: string
  subtype?: string
  message?: {
    id?: string
    content?: Array<Record<string, unknown>>
    usage?: { input_tokens?: number; output_tokens?: number }
    stop_reason?: string
  }
  tools?: unknown
  model?: string
  permissionMode?: string
  permission_mode?: string
  parent_tool_use_id?: string | null
  error?: string
  result?: string
  is_error?: boolean
  total_cost_usd?: number
  usage?: { input_tokens?: number; output_tokens?: number }
}

/** Small piece of mutable state the mapper uses across a single query() run. */
export interface MapperContext {
  conversationId: ConversationId
  /** Caller sets this when an assistant message is first observed. */
  currentMessageId: string | null
  /** Track subagent grouping when the SDK reports parent_tool_use_id. */
  subagentIds: Map<string, string>
}

export function createMapperContext(conversationId: ConversationId): MapperContext {
  return {
    conversationId,
    currentMessageId: null,
    subagentIds: new Map(),
  }
}

/**
 * Map a single SDK message into zero or more `SdkAgentEvent`s. The mapper is
 * intentionally synchronous — IO happens outside.
 */
export function mapSdkMessage(ctx: MapperContext, raw: SdkLikeMessage): SdkAgentEvent[] {
  return match(raw)
    .with({ type: 'system', subtype: 'init' }, (msg) => mapInit(ctx, msg))
    .with({ type: 'assistant' }, (msg) => mapAssistant(ctx, msg))
    .with({ type: 'user' }, (msg) => mapUserToolResults(ctx, msg))
    .with({ type: 'result' }, (msg) => mapResult(ctx, msg))
    .otherwise(() => [])
}

function mapInit(ctx: MapperContext, msg: SdkLikeMessage): SdkAgentEvent[] {
  if (!msg.session_id) return []
  const mode = (msg.permissionMode ?? msg.permission_mode ?? 'default') as PermissionMode
  return [
    {
      _tag: 'session.init',
      sessionId: ctx.conversationId,
      sdkSessionId: asSdkSessionId(msg.session_id) as SdkSessionId,
      model: msg.model ?? 'unknown',
      permissionMode: mode,
    },
  ]
}

function mapAssistant(ctx: MapperContext, msg: SdkLikeMessage): SdkAgentEvent[] {
  const content = msg.message?.content ?? []
  const messageId = msg.message?.id ?? msg.uuid ?? randomUUID()
  const typedMessageId = asMessageId(messageId)
  ctx.currentMessageId = messageId

  const contentBlocks: ContentBlock[] = []
  const toolEvents: SdkAgentEvent[] = []

  for (const block of content) {
    const b = block as { type?: string; [k: string]: unknown }
    match(b)
      .with({ type: 'text', text: P.string }, (t) => {
        contentBlocks.push({ type: 'text', text: t.text })
      })
      .with({ type: 'tool_use', id: P.string, name: P.string, input: P.any }, (t) => {
        contentBlocks.push({
          type: 'tool_use',
          id: t.id,
          name: t.name,
          input: (t.input ?? {}) as Record<string, unknown>,
        })
        toolEvents.push({
          _tag: 'tool.start',
          sessionId: ctx.conversationId,
          messageId: typedMessageId,
          toolEventId: t.id,
          name: t.name,
          input: (t.input ?? {}) as Record<string, unknown>,
        })
      })
      .otherwise(() => {
        // Unknown block — pass through as a best-effort text block so content
        // persistence stays lossless.
        if ('text' in b && typeof b.text === 'string') {
          contentBlocks.push({ type: 'text', text: b.text })
        }
      })
  }

  return [
    {
      _tag: 'assistant.message',
      sessionId: ctx.conversationId,
      messageId: typedMessageId,
      content: contentBlocks,
      tokensIn: msg.message?.usage?.input_tokens,
      tokensOut: msg.message?.usage?.output_tokens,
    },
    ...toolEvents,
  ]
}

function mapUserToolResults(ctx: MapperContext, msg: SdkLikeMessage): SdkAgentEvent[] {
  // User messages from the SDK can carry tool_result blocks when a tool
  // has finished running. We surface those as tool.result events.
  const content = msg.message?.content ?? []
  const events: SdkAgentEvent[] = []
  for (const block of content) {
    const b = block as {
      type?: string
      tool_use_id?: string
      content?: unknown
      is_error?: boolean
    }
    if (b.type !== 'tool_result' || typeof b.tool_use_id !== 'string') continue
    events.push({
      _tag: 'tool.result',
      sessionId: ctx.conversationId,
      toolEventId: b.tool_use_id,
      result: typeof b.content === 'string' ? b.content : JSON.stringify(b.content),
      isError: b.is_error === true,
      durationMs: 0,
    })
  }
  return events
}

function mapResult(ctx: MapperContext, msg: SdkLikeMessage): SdkAgentEvent[] {
  const events: SdkAgentEvent[] = []
  if (msg.usage) {
    events.push({
      _tag: 'usage',
      sessionId: ctx.conversationId,
      inputTokens: msg.usage.input_tokens ?? 0,
      outputTokens: msg.usage.output_tokens ?? 0,
      costUsd: msg.total_cost_usd,
    })
  }
  const isError = msg.subtype === 'error' || msg.is_error === true
  events.push({
    _tag: 'session.end',
    sessionId: ctx.conversationId,
    reason: isError ? 'error' : 'completed',
  })
  return events
}
