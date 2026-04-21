import { randomUUID } from 'crypto'
import type { ConversationId, SdkAgentEvent } from './types'
import type { SdkLikeMessage } from './eventMapper'
import { createMapperContext, mapSdkMessage, type MapperContext } from './eventMapper'

/**
 * Groups nested Task-tool runs into typed subagent.* events.
 *
 * The claude-agent-sdk surfaces sub-agent message streams with
 * `parent_tool_use_id` set on each nested `SDKMessage`. Messages without
 * that field belong to the outer conversation and are mapped as usual.
 *
 * Usage (eventMapper entry point wiring deferred — aggregator is a drop-in
 * replacement for a direct `mapSdkMessage` call when the SDK is confirmed
 * to emit nested Task streams in v0.2.x):
 *
 *   const agg = new SubagentAggregator(conversationId)
 *   for await (const raw of queryIter) {
 *     for (const ev of agg.observe(raw)) yield ev
 *   }
 */
export class SubagentAggregator {
  private readonly rootCtx: MapperContext
  /** Per-parent-tool-use-id context so nested streams build their own message chain. */
  private readonly nested = new Map<string, NestedSubagent>()

  constructor(private readonly conversationId: ConversationId) {
    this.rootCtx = createMapperContext(conversationId)
  }

  observe(message: SdkLikeMessage): SdkAgentEvent[] {
    const parentId =
      typeof message.parent_tool_use_id === 'string' ? message.parent_tool_use_id : null
    if (!parentId) {
      return mapSdkMessage(this.rootCtx, message)
    }

    const started = this.openIfNew(parentId, message)
    const nested = this.nested.get(parentId)
    if (!nested) return started

    const inner = mapSdkMessage(nested.ctx, message)
    const wrapped: SdkAgentEvent[] = []
    for (const ev of inner) {
      if (ev._tag === 'session.end') {
        wrapped.push({
          _tag: 'subagent.end',
          sessionId: this.conversationId,
          subagentId: nested.subagentId,
          status: ev.reason === 'completed' ? 'success' : 'error',
        })
        this.nested.delete(parentId)
        continue
      }
      wrapped.push({
        _tag: 'subagent.event',
        sessionId: this.conversationId,
        subagentId: nested.subagentId,
        event: ev,
      })
    }
    return [...started, ...wrapped]
  }

  private openIfNew(parentId: string, message: SdkLikeMessage): SdkAgentEvent[] {
    if (this.nested.has(parentId)) return []
    const subagentId = randomUUID()
    this.nested.set(parentId, {
      subagentId,
      parentToolUseId: parentId,
      ctx: createMapperContext(this.conversationId),
    })
    return [
      {
        _tag: 'subagent.start',
        sessionId: this.conversationId,
        subagentId,
        task: deriveTask(message),
        agentType: 'claude-sdk',
      },
    ]
  }
}

interface NestedSubagent {
  subagentId: string
  parentToolUseId: string
  ctx: MapperContext
}

function deriveTask(message: SdkLikeMessage): string {
  // Task-tool invocations carry the prompt in the outer tool input. Fall
  // back to an opaque label until we wire the outer event to the nested
  // stream (requires correlating on parent_tool_use_id <-> tool_use.id).
  return (message.message?.stop_reason as string | undefined) ?? 'Sub-agent task'
}
