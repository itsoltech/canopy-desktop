import type { ResultAsync } from 'neverthrow'
import type {
  Attachment,
  ContentBlock,
  ConversationId,
  EffortLevel,
  PermissionMode,
  SdkAgentEvent,
} from '../types'
import type { SdkAgentError } from '../errors'

/**
 * Provider-agnostic contract for LLM backends. Only `AnthropicProvider` is
 * implemented in MVP (see Phase 3); this interface exists from day one so
 * `SdkAgentManager` stays decoupled from vendor-specific types.
 */
export type ProviderId = 'anthropic' | 'openai' | 'gemini'

export interface ProviderToolContext {
  /** Forwarded from the manager. Providers delegate permission prompts here. */
  canUseTool: CanUseToolCallback
  /** AbortController.signal for the in-flight query. */
  signal: AbortSignal
}

/**
 * Provider callback invoked before a tool runs. Mirrors the SDK's `canUseTool`
 * contract so `AnthropicProvider` can forward straight through.
 */
export type CanUseToolCallback = (
  toolName: string,
  input: Record<string, unknown>,
  ctx: { signal: AbortSignal; suggestions?: ReadonlyArray<unknown>; toolUseId?: string },
) => Promise<
  | { behavior: 'allow'; updatedInput: Record<string, unknown> }
  | { behavior: 'deny'; message?: string }
>

export interface ProviderQueryOptions {
  conversationId: ConversationId
  prompt: string | ContentBlock[]
  attachments?: Attachment[]
  model: string
  permissionMode: PermissionMode
  /**
   * Optional reasoning-effort control. Only forwarded when the selected
   * model advertises reasoning support. `null`/`undefined` leaves the
   * SDK default in place.
   */
  effort?: EffortLevel | null
  appendSystemPrompt?: string
  cwd: string
  mcpServers?: Record<string, unknown>
  apiKey?: string
  baseUrl?: string
  customEnv?: string
  settingsJson?: string
  approvalMode?: string
  sandboxMode?: string
  context: ProviderToolContext
  /** SDK session id for resume (omit on first message). */
  resume?: string
}

export interface LlmProvider {
  readonly providerId: ProviderId
  query(options: ProviderQueryOptions): ResultAsync<AsyncIterable<SdkAgentEvent>, SdkAgentError>
}
