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
 * The `subagentId` in the emitted events is the parent tool_use_id, so the
 * renderer can correlate the top-level Task/Agent `tool.start` with the
 * nested stream it spawned.
 */
export class SubagentAggregator {
  private readonly rootCtx: MapperContext
  /** Per-parent-tool-use-id context so nested streams build their own message chain. */
  private readonly nested = new Map<string, NestedSubagent>()
  /** Captured inputs for Task/Agent tool invocations at the root so subagent.start can carry task/agentType. */
  private readonly rootToolUses = new Map<string, { task: string; agentType: string }>()

  constructor(private readonly conversationId: ConversationId) {
    this.rootCtx = createMapperContext(conversationId)
  }

  observe(message: SdkLikeMessage): SdkAgentEvent[] {
    const parentId =
      typeof message.parent_tool_use_id === 'string' ? message.parent_tool_use_id : null

    if (!parentId) {
      const events = mapSdkMessage(this.rootCtx, message)
      const out: SdkAgentEvent[] = []
      for (const ev of events) {
        out.push(ev)
        if (ev._tag === 'tool.start' && isSubagentTool(ev.name)) {
          this.rootToolUses.set(ev.toolEventId, {
            task: extractTaskLabel(ev.input),
            agentType: extractAgentType(ev.input),
          })
        }
        if (ev._tag === 'tool.result' && this.nested.has(ev.toolEventId)) {
          out.push({
            _tag: 'subagent.end',
            sessionId: this.conversationId,
            subagentId: ev.toolEventId,
            status: ev.isError ? 'error' : 'success',
          })
          this.nested.delete(ev.toolEventId)
        }
      }
      return out
    }

    const started = this.openIfNew(parentId)
    const nested = this.nested.get(parentId)
    if (!nested) return started

    const inner = mapSdkMessage(nested.ctx, message)
    const wrapped: SdkAgentEvent[] = []
    for (const ev of inner) {
      // Subagent's own session lifecycle events aren't meaningful to the
      // renderer — we gate start/end on root-level tool.start/result.
      if (ev._tag === 'session.init' || ev._tag === 'session.end') continue
      wrapped.push({
        _tag: 'subagent.event',
        sessionId: this.conversationId,
        subagentId: parentId,
        event: ev,
      })
    }
    return [...started, ...wrapped]
  }

  private openIfNew(parentId: string): SdkAgentEvent[] {
    if (this.nested.has(parentId)) return []
    const cached = this.rootToolUses.get(parentId)
    this.nested.set(parentId, {
      subagentId: parentId,
      parentToolUseId: parentId,
      ctx: createMapperContext(this.conversationId),
    })
    return [
      {
        _tag: 'subagent.start',
        sessionId: this.conversationId,
        subagentId: parentId,
        task: cached?.task ?? 'Sub-agent task',
        agentType: cached?.agentType ?? 'general',
      },
    ]
  }
}

interface NestedSubagent {
  subagentId: string
  parentToolUseId: string
  ctx: MapperContext
}

function isSubagentTool(name: string): boolean {
  return name === 'Task' || name === 'Agent'
}

function extractTaskLabel(input: Record<string, unknown>): string {
  if (typeof input.description === 'string' && input.description.trim()) return input.description
  if (typeof input.prompt === 'string' && input.prompt.trim()) {
    return input.prompt.length > 120 ? input.prompt.slice(0, 120) + '…' : input.prompt
  }
  return 'Sub-agent task'
}

function extractAgentType(input: Record<string, unknown>): string {
  if (typeof input.subagent_type === 'string' && input.subagent_type.trim()) {
    return input.subagent_type
  }
  return 'general'
}
