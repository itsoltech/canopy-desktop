import { randomUUID } from 'crypto'
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
function wrapCanUseTool(inner: CanUseToolCallback) {
  return async (
    toolName: string,
    input: Record<string, unknown>,
    options: { signal: AbortSignal; suggestions?: ReadonlyArray<unknown> },
  ) => {
    const result = await inner(toolName, input, {
      signal: options.signal,
      suggestions: options.suggestions,
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

        const q = query({
          prompt,
          options: {
            model: options.model,
            cwd: options.cwd,
            permissionMode: options.permissionMode,
            appendSystemPrompt: options.appendSystemPrompt,
            mcpServers: mcp,
            pathToClaudeCodeExecutable: claudePath,
            canUseTool: wrapCanUseTool(options.context.canUseTool),
            resume: options.resume,
            abortController: toLegacyAbortController(options.context.signal),
          } as Parameters<typeof query>[0]['options'],
        })

        async function* translate(): AsyncIterable<SdkAgentEvent> {
          for await (const message of q) {
            const events = mapSdkMessage(ctx, message as unknown as SdkLikeMessage)
            for (const ev of events) yield ev
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
  return {
    behavior: 'allow',
    updatedInput: {
      ...input,
      answers,
      __canopyResponseId: randomUUID(),
    },
  }
}
