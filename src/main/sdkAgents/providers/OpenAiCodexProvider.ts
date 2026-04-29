import util from 'util'
import type {
  ApprovalMode,
  Codex,
  Input,
  ModelReasoningEffort,
  SandboxMode,
  ThreadEvent,
  ThreadItem,
} from '@openai/codex-sdk'
import { ResultAsync } from 'neverthrow'
import { randomUUID } from 'crypto'
import type { Attachment, ContentBlock, EffortLevel, SdkAgentEvent } from '../types'
import { asMessageId, asSdkSessionId } from '../types'
import type { SdkAgentError } from '../errors'
import { toSdkAgentError } from '../errors'
import type { LlmProvider, ProviderId, ProviderQueryOptions } from './LlmProvider'
import { BLOCKED_ENV_VARS } from '../../security/envBlocklist'

const DEFAULT_MODEL_LABEL = 'codex'
const INTERNAL_BLOCKED_ENV = new Set(['CANOPY_HOOK_PORT', 'CANOPY_HOOK_TOKEN'])

const INSPECT_OPTIONS: util.InspectOptions = {
  depth: 8,
  breakLength: 120,
  maxStringLength: 2000,
  maxArrayLength: 200,
  compact: 4,
  colors: false,
}

function log(message: string, details: Record<string, unknown> = {}): void {
  console.info('[sdk-agent][openai-codex]', message, util.inspect(details, INSPECT_OPTIONS))
}

export class OpenAiCodexProvider implements LlmProvider {
  readonly providerId: ProviderId = 'openai'

  query(options: ProviderQueryOptions): ResultAsync<AsyncIterable<SdkAgentEvent>, SdkAgentError> {
    return ResultAsync.fromPromise(
      (async (): Promise<AsyncIterable<SdkAgentEvent>> => {
        const codex = new (await loadCodex())({
          ...(options.apiKey ? { apiKey: options.apiKey } : {}),
          ...(options.baseUrl ? { baseUrl: options.baseUrl } : {}),
          env: buildEnv(options.customEnv),
          config: parseConfig(options.settingsJson),
        })
        const threadOptions = {
          ...(modelForSdk(options.model) ? { model: modelForSdk(options.model) } : {}),
          ...(toSandboxMode(options.sandboxMode)
            ? { sandboxMode: toSandboxMode(options.sandboxMode) }
            : {}),
          ...(toApprovalMode(options.approvalMode)
            ? { approvalPolicy: toApprovalMode(options.approvalMode) }
            : {}),
          ...(toReasoningEffort(options.effort)
            ? { modelReasoningEffort: toReasoningEffort(options.effort) }
            : {}),
          workingDirectory: options.cwd,
          skipGitRepoCheck: true,
        }
        const thread = options.resume
          ? codex.resumeThread(options.resume, threadOptions)
          : codex.startThread(threadOptions)
        const input = toCodexInput(options.prompt, options.attachments)

        log('query:start', {
          conversationId: options.conversationId,
          model: options.model,
          cwd: options.cwd,
          resume: options.resume ?? null,
          attachmentCount: options.attachments?.length ?? 0,
        })

        const { events } = await thread.runStreamed(input, { signal: options.context.signal })
        return translateEvents(options, events)
      })(),
      (e) => toSdkAgentError(e),
    )
  }
}

async function loadCodex(): Promise<typeof Codex> {
  const mod = await import('@openai/codex-sdk')
  return mod.Codex
}

function translateEvents(
  options: ProviderQueryOptions,
  events: AsyncGenerator<ThreadEvent>,
): AsyncIterable<SdkAgentEvent> {
  const textByItemId = new Map<string, string>()
  const thinkingByItemId = new Map<string, string>()
  const startedTools = new Set<string>()
  let currentMessageId = asMessageId(`codex-${randomUUID()}`)
  const startedAt = Date.now()

  async function* translate(): AsyncIterable<SdkAgentEvent> {
    try {
      for await (const event of events) {
        log('query:stream:event', {
          conversationId: options.conversationId,
          event: summarizeEvent(event),
        })
        if (event.type === 'thread.started') {
          yield {
            _tag: 'session.init',
            sessionId: options.conversationId,
            sdkSessionId: asSdkSessionId(event.thread_id),
            model: options.model || DEFAULT_MODEL_LABEL,
            permissionMode: options.permissionMode,
          }
          continue
        }

        if (event.type === 'item.started' || event.type === 'item.updated') {
          const itemEvents = mapItemProgress(options, event.item, {
            currentMessageId,
            textByItemId,
            thinkingByItemId,
            startedTools,
          })
          for (const ev of itemEvents) yield ev
          continue
        }

        if (event.type === 'item.completed') {
          const completed = mapItemCompleted(options, event.item, {
            currentMessageId,
            textByItemId,
            thinkingByItemId,
            startedTools,
          })
          for (const ev of completed.events) yield ev
          if (completed.nextMessageId) currentMessageId = completed.nextMessageId
          continue
        }

        if (event.type === 'turn.completed') {
          yield {
            _tag: 'usage',
            sessionId: options.conversationId,
            inputTokens: event.usage.input_tokens,
            outputTokens: event.usage.output_tokens,
            cacheReadInputTokens: event.usage.cached_input_tokens,
            durationMs: Date.now() - startedAt,
          }
          yield {
            _tag: 'session.end',
            sessionId: options.conversationId,
            reason: 'completed',
          }
          continue
        }

        if (event.type === 'turn.failed') {
          yield {
            _tag: 'error',
            sessionId: options.conversationId,
            error: { _tag: 'sdk_internal', message: event.error.message },
          }
          yield { _tag: 'session.end', sessionId: options.conversationId, reason: 'error' }
          continue
        }

        if (event.type === 'error') {
          yield {
            _tag: 'error',
            sessionId: options.conversationId,
            error: { _tag: 'sdk_internal', message: event.message },
          }
          yield { _tag: 'session.end', sessionId: options.conversationId, reason: 'error' }
        }
      }
    } catch (e) {
      log('query:stream:error', {
        conversationId: options.conversationId,
        error: e instanceof Error ? e.message : String(e),
      })
      throw e
    }
  }

  return translate()
}

interface ItemMapperState {
  currentMessageId: ReturnType<typeof asMessageId>
  textByItemId: Map<string, string>
  thinkingByItemId: Map<string, string>
  startedTools: Set<string>
}

function mapItemProgress(
  options: ProviderQueryOptions,
  item: ThreadItem,
  state: ItemMapperState,
): SdkAgentEvent[] {
  if (item.type === 'agent_message') {
    const previous = state.textByItemId.get(item.id) ?? ''
    const delta = item.text.slice(previous.length)
    state.textByItemId.set(item.id, item.text)
    if (!delta) return []
    return [
      {
        _tag: 'assistant.delta',
        sessionId: options.conversationId,
        messageId: state.currentMessageId,
        text: delta,
      },
    ]
  }

  if (item.type === 'reasoning') {
    const previous = state.thinkingByItemId.get(item.id) ?? ''
    const delta = item.text.slice(previous.length)
    state.thinkingByItemId.set(item.id, item.text)
    if (!delta) return []
    return [
      {
        _tag: 'assistant.thinking',
        sessionId: options.conversationId,
        messageId: state.currentMessageId,
        text: delta,
      },
    ]
  }

  return maybeStartTool(options, item, state)
}

function mapItemCompleted(
  options: ProviderQueryOptions,
  item: ThreadItem,
  state: ItemMapperState,
): { events: SdkAgentEvent[]; nextMessageId?: ReturnType<typeof asMessageId> } {
  if (item.type === 'error') {
    return {
      events: [
        {
          _tag: 'error',
          sessionId: options.conversationId,
          error: { _tag: 'sdk_internal', message: item.message },
        },
      ],
    }
  }

  if (item.type === 'agent_message') {
    state.textByItemId.set(item.id, item.text)
    const nextMessageId = asMessageId(`codex-${randomUUID()}`)
    return {
      events: [
        {
          _tag: 'assistant.message',
          sessionId: options.conversationId,
          messageId: state.currentMessageId,
          content: [{ type: 'text', text: item.text }],
          model: modelForDisplay(options.model),
        },
      ],
      nextMessageId,
    }
  }

  if (item.type === 'reasoning') {
    state.thinkingByItemId.set(item.id, item.text)
    return { events: [] }
  }

  const events = maybeStartTool(options, item, state)
  const result = toolResultForItem(options, item)
  if (result) events.push(result)
  return { events }
}

function maybeStartTool(
  options: ProviderQueryOptions,
  item: ThreadItem,
  state: ItemMapperState,
): SdkAgentEvent[] {
  const tool = toolStartForItem(options, item, state.currentMessageId)
  if (!tool || state.startedTools.has(tool.toolEventId)) return []
  state.startedTools.add(tool.toolEventId)
  return [tool]
}

function toolStartForItem(
  options: ProviderQueryOptions,
  item: ThreadItem,
  messageId: ReturnType<typeof asMessageId>,
): Extract<SdkAgentEvent, { _tag: 'tool.start' }> | null {
  if (item.type === 'command_execution') {
    return {
      _tag: 'tool.start',
      sessionId: options.conversationId,
      messageId,
      toolEventId: item.id,
      name: 'command_execution',
      input: { command: item.command },
    }
  }
  if (item.type === 'mcp_tool_call') {
    return {
      _tag: 'tool.start',
      sessionId: options.conversationId,
      messageId,
      toolEventId: item.id,
      name: `${item.server}.${item.tool}`,
      input: (item.arguments && typeof item.arguments === 'object'
        ? item.arguments
        : { arguments: item.arguments }) as Record<string, unknown>,
    }
  }
  if (item.type === 'file_change') {
    return {
      _tag: 'tool.start',
      sessionId: options.conversationId,
      messageId,
      toolEventId: item.id,
      name: 'file_change',
      input: { changes: item.changes },
    }
  }
  if (item.type === 'web_search') {
    return {
      _tag: 'tool.start',
      sessionId: options.conversationId,
      messageId,
      toolEventId: item.id,
      name: 'web_search',
      input: { query: item.query },
    }
  }
  if (item.type === 'todo_list') {
    return {
      _tag: 'tool.start',
      sessionId: options.conversationId,
      messageId,
      toolEventId: item.id,
      name: 'todo_list',
      input: { items: item.items },
    }
  }
  return null
}

function toolResultForItem(
  options: ProviderQueryOptions,
  item: ThreadItem,
): Extract<SdkAgentEvent, { _tag: 'tool.result' }> | null {
  if (item.type === 'command_execution') {
    return {
      _tag: 'tool.result',
      sessionId: options.conversationId,
      toolEventId: item.id,
      result: item.aggregated_output,
      isError: item.status === 'failed' || (item.exit_code !== undefined && item.exit_code !== 0),
      durationMs: 0,
    }
  }
  if (item.type === 'mcp_tool_call') {
    return {
      _tag: 'tool.result',
      sessionId: options.conversationId,
      toolEventId: item.id,
      result: item.error?.message ?? stringifyMcpResult(item.result),
      isError: item.status === 'failed',
      durationMs: 0,
    }
  }
  if (item.type === 'file_change') {
    return {
      _tag: 'tool.result',
      sessionId: options.conversationId,
      toolEventId: item.id,
      result: item.changes.map((change) => `${change.kind}: ${change.path}`).join('\n'),
      isError: item.status === 'failed',
      durationMs: 0,
    }
  }
  if (item.type === 'web_search') {
    return {
      _tag: 'tool.result',
      sessionId: options.conversationId,
      toolEventId: item.id,
      result: `Search completed: ${item.query}`,
      isError: false,
      durationMs: 0,
    }
  }
  if (item.type === 'todo_list') {
    return {
      _tag: 'tool.result',
      sessionId: options.conversationId,
      toolEventId: item.id,
      result: item.items.map((todo) => `${todo.completed ? '[x]' : '[ ]'} ${todo.text}`).join('\n'),
      isError: false,
      durationMs: 0,
    }
  }
  return null
}

function toCodexInput(prompt: string | ContentBlock[], attachments: Attachment[] = []): Input {
  if (typeof prompt === 'string') return prompt
  const input: Input = []
  for (const block of prompt) {
    if (block.type === 'text') input.push({ type: 'text', text: block.text })
  }
  for (const attachment of attachments) {
    if (attachment.kind === 'image') input.push({ type: 'local_image', path: attachment.path })
  }
  return input.length > 0 ? input : ''
}

function modelForSdk(model: string | undefined): string | undefined {
  const trimmed = model?.trim()
  if (!trimmed || trimmed === DEFAULT_MODEL_LABEL) return undefined
  return trimmed
}

function modelForDisplay(model: string | undefined): string {
  return modelForSdk(model) ?? DEFAULT_MODEL_LABEL
}

function toApprovalMode(value: string | undefined): ApprovalMode | undefined {
  if (
    value === 'never' ||
    value === 'on-request' ||
    value === 'on-failure' ||
    value === 'untrusted'
  ) {
    return value
  }
  return undefined
}

function toSandboxMode(value: string | undefined): SandboxMode | undefined {
  if (value === 'read-only' || value === 'workspace-write' || value === 'danger-full-access') {
    return value
  }
  return undefined
}

function toReasoningEffort(
  effort: EffortLevel | null | undefined,
): ModelReasoningEffort | undefined {
  if (effort === 'low' || effort === 'medium' || effort === 'high') return effort
  if (effort === 'max') return 'xhigh'
  return undefined
}

function parseConfig(raw: string | undefined): Record<string, never> | undefined {
  if (!raw?.trim()) return undefined
  try {
    const parsed = JSON.parse(raw) as unknown
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, never>
    }
  } catch {
    // Invalid user JSON is ignored to match existing profile behavior.
  }
  return undefined
}

function buildEnv(customEnv: string | undefined): Record<string, string> {
  const env: Record<string, string> = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined) env[key] = value
  }

  if (!customEnv?.trim()) return env
  try {
    const parsed = JSON.parse(customEnv) as unknown
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return env
    for (const [key, value] of Object.entries(parsed)) {
      const normalized = key.toUpperCase()
      if (
        typeof value === 'string' &&
        !BLOCKED_ENV_VARS.has(normalized) &&
        !INTERNAL_BLOCKED_ENV.has(normalized)
      ) {
        env[key] = value
      }
    }
  } catch {
    // Invalid user JSON is ignored to match existing profile behavior.
  }
  return env
}

function stringifyMcpResult(
  result: { content: unknown[]; structured_content: unknown } | undefined,
): string {
  if (!result) return ''
  const textParts = result.content.flatMap((block) => {
    if (!block || typeof block !== 'object') return []
    const record = block as { type?: string; text?: string }
    return record.type === 'text' && typeof record.text === 'string' ? [record.text] : []
  })
  if (textParts.length > 0) return textParts.join('\n\n')
  return JSON.stringify(result.structured_content ?? result.content)
}

function summarizeEvent(event: ThreadEvent): unknown {
  if (
    event.type === 'item.started' ||
    event.type === 'item.updated' ||
    event.type === 'item.completed'
  ) {
    return { type: event.type, item: { id: event.item.id, type: event.item.type } }
  }
  return event
}
