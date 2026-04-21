/**
 * Foundational types for the SDK-agent backend. Freeze these early — DB schema,
 * IPC serialization, and the renderer store all read from this file.
 *
 * See SPEC.md at the project root for the full design. This file is deliberately
 * SDK-agnostic: it describes the wire shape our own services produce after mapping
 * SDK messages. No `@anthropic-ai/claude-agent-sdk` imports here — that coupling
 * lives in `providers/AnthropicProvider.ts` (Phase 3).
 */

declare const conversationIdBrand: unique symbol
declare const messageIdBrand: unique symbol
declare const sdkSessionIdBrand: unique symbol

export type ConversationId = string & { readonly [conversationIdBrand]: true }
export type MessageId = string & { readonly [messageIdBrand]: true }
export type SdkSessionId = string & { readonly [sdkSessionIdBrand]: true }

export const asConversationId = (id: string): ConversationId => id as ConversationId
export const asMessageId = (id: string): MessageId => id as MessageId
export const asSdkSessionId = (id: string): SdkSessionId => id as SdkSessionId

/**
 * Canonical content shape we persist alongside assistant / user messages.
 * This is our own shape (not an SDK re-export) so renderer + IPC boundaries
 * don't need the SDK as a type-only dependency.
 */
export type ContentBlock =
  | { type: 'text'; text: string }
  | {
      type: 'image'
      source: { type: 'base64'; media_type: string; data: string }
      filename?: string
    }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, unknown> }
  | {
      type: 'tool_result'
      tool_use_id: string
      content: string
      is_error?: boolean
    }

export type AttachmentKind = 'image' | 'text'

export interface Attachment {
  id: string
  kind: AttachmentKind
  filename: string
  path: string
  mimeType: string
  sizeBytes: number
}

/** AskUserQuestion shape — mirrors the SDK tool's public contract. */
export interface QuestionOption {
  label: string
  description?: string
  preview?: string
}

export interface Question {
  header: string
  question: string
  options: QuestionOption[]
  multiSelect?: boolean
}

export interface AskUserQuestionAnswer {
  selected: string[]
  other?: string
  notes?: string
}

export type ToolDecision = 'allow-once' | 'allow-session' | 'deny'

export type PlanDecision = { action: 'approve' } | { action: 'reject'; feedback?: string }

export interface PlanAllowedPrompt {
  tool: string
  prompt: string
}

/** Permission-mode aliases mirror SDK options. */
export type PermissionMode = 'default' | 'plan' | 'acceptEdits' | 'bypassPermissions'

/**
 * Discriminated union of everything a session can emit. Consumers (persistence,
 * IPC push channel, renderer reducer) switch on `_tag` with ts-pattern
 * `.exhaustive()` so new kinds force updates across the stack.
 */
export type SdkAgentEvent =
  | {
      _tag: 'session.init'
      sessionId: ConversationId
      sdkSessionId: SdkSessionId
      model: string
      permissionMode: PermissionMode
    }
  | {
      _tag: 'assistant.delta'
      sessionId: ConversationId
      messageId: MessageId
      text: string
    }
  | {
      _tag: 'assistant.thinking'
      sessionId: ConversationId
      messageId: MessageId
      text: string
    }
  | {
      _tag: 'assistant.message'
      sessionId: ConversationId
      messageId: MessageId
      content: ContentBlock[]
      tokensIn?: number
      tokensOut?: number
      costUsd?: number
      /** The model that actually produced this message, when the SDK reports it. */
      model?: string
    }
  | {
      _tag: 'tool.start'
      sessionId: ConversationId
      messageId: MessageId
      toolEventId: string
      name: string
      input: Record<string, unknown>
    }
  | {
      _tag: 'tool.result'
      sessionId: ConversationId
      toolEventId: string
      result: string
      isError: boolean
      durationMs: number
    }
  | {
      _tag: 'tool.permission_request'
      sessionId: ConversationId
      requestId: string
      toolName: string
      input: Record<string, unknown>
    }
  | {
      _tag: 'ask_user_question'
      sessionId: ConversationId
      requestId: string
      questions: Question[]
    }
  | {
      _tag: 'plan_mode_exit'
      sessionId: ConversationId
      requestId: string
      toolEventId?: string
      plan: string
      allowedPrompts?: PlanAllowedPrompt[]
    }
  | {
      _tag: 'subagent.start'
      sessionId: ConversationId
      subagentId: string
      task: string
      agentType: string
    }
  | {
      _tag: 'subagent.event'
      sessionId: ConversationId
      subagentId: string
      event: SdkAgentEvent
    }
  | {
      _tag: 'subagent.end'
      sessionId: ConversationId
      subagentId: string
      status: 'success' | 'error'
      summary?: string
    }
  | {
      _tag: 'usage'
      sessionId: ConversationId
      inputTokens: number
      outputTokens: number
      costUsd?: number
    }
  | {
      _tag: 'error'
      sessionId: ConversationId
      error: import('./errors').SdkAgentError
    }
  | {
      _tag: 'session.end'
      sessionId: ConversationId
      reason: 'completed' | 'aborted' | 'error'
    }

export type SdkAgentEventTag = SdkAgentEvent['_tag']
