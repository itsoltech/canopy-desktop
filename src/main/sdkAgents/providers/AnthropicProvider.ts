import { execFile } from 'child_process'
import os from 'os'
import { query } from '@anthropic-ai/claude-agent-sdk'
import { ResultAsync } from 'neverthrow'
import type { AskUserQuestionAnswer, Question, SdkAgentEvent } from '../types'
import type { SdkAgentError } from '../errors'
import { toSdkAgentError } from '../errors'
import type {
  CanUseToolCallback,
  LlmProvider,
  ProviderId,
  ProviderQueryOptions,
} from './LlmProvider'
import type { SdkLikeMessage } from '../eventMapper'
import { createMapperContext, mapSdkMessage } from '../eventMapper'

let cachedClaudePath: string | undefined

function log(message: string, details: Record<string, unknown> = {}): void {
  console.info('[sdk-agent][anthropic]', message, details)
}

async function resolveClaudeExecutable(): Promise<string | undefined> {
  if (cachedClaudePath !== undefined) return cachedClaudePath || undefined
  const cmd = os.platform() === 'win32' ? 'where' : 'which'
  return new Promise((resolve) => {
    execFile(cmd, ['claude'], { env: process.env }, (err, stdout) => {
      cachedClaudePath = err ? '' : stdout.trim()
      resolve(cachedClaudePath || undefined)
    })
  })
}

/**
 * Build the SDK's canUseTool from our provider-agnostic CanUseToolCallback.
 * The manager supplies the outer callback; all this does is translate
 * between the two shapes.
 */
function wrapCanUseTool(inner: CanUseToolCallback, conversationId: string) {
  return async (
    toolName: string,
    input: Record<string, unknown>,
    options: {
      signal: AbortSignal
      suggestions?: ReadonlyArray<unknown>
      toolUseID?: string
    },
  ) => {
    log('canUseTool:start', {
      conversationId,
      toolName,
      toolUseID: options.toolUseID,
      input: summarizeToolInput(toolName, input),
    })
    const result = await inner(toolName, input, {
      signal: options.signal,
      suggestions: options.suggestions,
    })
    log('canUseTool:result', {
      conversationId,
      toolName,
      behavior: result.behavior,
      updatedInput:
        result.behavior === 'allow' ? summarizeToolInput(toolName, result.updatedInput) : null,
    })
    if (result.behavior === 'allow') {
      return {
        behavior: 'allow' as const,
        updatedInput: result.updatedInput,
      }
    }
    return {
      behavior: 'deny' as const,
      message: result.message ?? 'Denied by user.',
    }
  }
}

export class AnthropicProvider implements LlmProvider {
  readonly providerId: ProviderId = 'anthropic'

  query(options: ProviderQueryOptions): ResultAsync<AsyncIterable<SdkAgentEvent>, SdkAgentError> {
    return ResultAsync.fromPromise(
      (async (): Promise<AsyncIterable<SdkAgentEvent>> => {
        const claudePath = await resolveClaudeExecutable()
        const ctx = createMapperContext(options.conversationId)

        const mcp =
          options.mcpServers && Object.keys(options.mcpServers).length > 0
            ? (options.mcpServers as Record<string, never>)
            : undefined

        const prompt = typeof options.prompt === 'string' ? options.prompt : ''
        log('query:start', {
          conversationId: options.conversationId,
          model: options.model,
          permissionMode: options.permissionMode,
          cwd: options.cwd,
          resume: options.resume ?? null,
          mcpServerCount: mcp ? Object.keys(mcp).length : 0,
          attachmentCount: options.attachments?.length ?? 0,
          promptLength: prompt.length,
          claudePath: claudePath ?? null,
        })

        const q = query({
          prompt,
          options: {
            model: options.model,
            cwd: options.cwd,
            permissionMode: options.permissionMode,
            appendSystemPrompt: options.appendSystemPrompt,
            mcpServers: mcp,
            pathToClaudeCodeExecutable: claudePath,
            canUseTool: wrapCanUseTool(options.context.canUseTool, options.conversationId),
            resume: options.resume,
            abortController: toLegacyAbortController(options.context.signal),
          } as Parameters<typeof query>[0]['options'],
        })

        async function* translate(): AsyncIterable<SdkAgentEvent> {
          log('query:stream:start', { conversationId: options.conversationId })
          try {
            for await (const message of q) {
              const raw = message as unknown as SdkLikeMessage
              log('query:stream:message', {
                conversationId: options.conversationId,
                message: summarizeSdkMessage(raw),
              })
              const events = mapSdkMessage(ctx, raw)
              for (const ev of events) {
                log('query:stream:event', {
                  conversationId: options.conversationId,
                  event: summarizeEvent(ev),
                })
                yield ev
              }
            }
            log('query:stream:end', { conversationId: options.conversationId })
          } catch (e) {
            log('query:stream:error', {
              conversationId: options.conversationId,
              error: e instanceof Error ? e.message : String(e),
            })
            throw e
          }
        }

        return translate()
      })(),
      (e) => toSdkAgentError(e),
    )
  }
}

/**
 * Some SDK versions expect a full AbortController rather than just a signal.
 * Wrap an upstream signal so aborting the outer controller propagates.
 */
function toLegacyAbortController(signal: AbortSignal): AbortController {
  const controller = new AbortController()
  if (signal.aborted) controller.abort()
  else signal.addEventListener('abort', () => controller.abort(), { once: true })
  return controller
}

/**
 * Translates our session-scoped inbound responses back into `canUseTool`
 * results. Importers own the pending-promise map; this helper just shapes
 * the payload each kind expects.
 */
export interface AskUserQuestionInput {
  questions: Question[]
}

export function shapeQuestionAllowPayload(
  input: Record<string, unknown>,
  answers: Record<string, AskUserQuestionAnswer>,
): { behavior: 'allow'; updatedInput: Record<string, unknown> } {
  const questions = Array.isArray(input.questions) ? (input.questions as Question[]) : []
  const formattedAnswers: Record<string, string> = {}
  const annotations: Record<string, { preview?: string; notes?: string }> = {}

  for (const [questionText, answer] of Object.entries(answers)) {
    formattedAnswers[questionText] = stringifyAnswer(answer)

    const question = questions.find((q) => q.question === questionText)
    const preview = question?.options.find(
      (option) => answer.selected.includes(option.label) && option.preview,
    )?.preview
    const notes = answer.notes?.trim()
    if (preview || notes) {
      annotations[questionText] = {
        ...(preview ? { preview } : {}),
        ...(notes ? { notes } : {}),
      }
    }
  }

  return {
    behavior: 'allow',
    updatedInput: {
      ...input,
      answers: formattedAnswers,
      ...(Object.keys(annotations).length > 0 ? { annotations } : {}),
    },
  }
}

function stringifyAnswer(answer: AskUserQuestionAnswer): string {
  return answer.selected
    .map((label) => {
      if (label === 'Other' && answer.other?.trim()) return answer.other.trim()
      return label
    })
    .join(', ')
}

function summarizeToolInput(toolName: string, input: Record<string, unknown> | undefined): unknown {
  if (!input) return null
  if (toolName === 'AskUserQuestion') {
    const questions = Array.isArray(input.questions) ? (input.questions as Question[]) : []
    const answers =
      input.answers && typeof input.answers === 'object'
        ? Object.keys(input.answers as Record<string, unknown>)
        : []
    return {
      questionCount: questions.length,
      questions: questions.map((q) => ({
        header: q.header,
        question: q.question,
        optionCount: q.options.length,
        hasPreview: q.options.some((option) => !!option.preview),
        multiSelect: q.multiSelect === true,
      })),
      answerKeys: answers,
    }
  }
  return {
    keys: Object.keys(input),
  }
}

function summarizeSdkMessage(message: SdkLikeMessage): Record<string, unknown> {
  const blocks = message.message?.content?.map((block) => {
    const type = typeof block.type === 'string' ? block.type : 'unknown'
    return {
      type,
      name: typeof block.name === 'string' ? block.name : undefined,
      id: typeof block.id === 'string' ? block.id : undefined,
      textLength: typeof block.text === 'string' ? block.text.length : undefined,
    }
  })
  return {
    type: message.type,
    subtype: message.subtype,
    sessionId: message.session_id,
    uuid: message.uuid,
    blockCount: blocks?.length ?? 0,
    blocks,
    isError: message.is_error,
  }
}

function summarizeEvent(event: SdkAgentEvent): Record<string, unknown> {
  return {
    tag: event._tag,
    toolName: 'name' in event ? event.name : 'toolName' in event ? event.toolName : undefined,
    requestId: 'requestId' in event ? event.requestId : undefined,
    reason: 'reason' in event ? event.reason : undefined,
  }
}
