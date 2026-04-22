import { match } from 'ts-pattern'

/**
 * Reactive per-conversation view of the SDK-agent backend. Renderer panes
 * read from this store; they never talk to main directly. Events are
 * reduced via ts-pattern on the event `_tag` so adding a new event kind
 * is a compile-time push.
 */

export type AttentionBlock =
  | {
      kind: 'question'
      requestId: string
      messageId: string | null
      questions: SdkAgentEvent extends { _tag: 'ask_user_question'; questions: infer Q } ? Q : never
      status: 'waiting' | 'resolved' | 'cancelled'
      answers?: Record<string, SdkAskUserQuestionAnswer>
    }
  | {
      kind: 'plan'
      requestId: string
      messageId: string | null
      toolEventId?: string
      plan: string
      permissionMode?: Exclude<SdkPermissionMode, 'plan'>
      allowedPrompts?: SdkAgentEvent extends { _tag: 'plan_mode_exit'; allowedPrompts?: infer P }
        ? P
        : never
      status: 'waiting' | 'approved' | 'rejected'
      feedback?: string
    }
  | {
      kind: 'permission'
      requestId: string
      messageId: string | null
      toolName: string
      input: Record<string, unknown>
      status: 'waiting' | 'granted' | 'denied'
    }

type QuestionAttentionBlock = Extract<AttentionBlock, { kind: 'question' }>
type QuestionSpec = QuestionAttentionBlock['questions'][number]

export interface SdkMessageView {
  id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  thinking: string
  contentBlocks: Array<Record<string, unknown>>
  tokensIn: number | null
  tokensOut: number | null
  costUsd: number | null
  /** Wall-clock ms between the last user send and this assistant message. Null for user/tool/system, or if we weren't watching. */
  elapsedMs: number | null
  /** Cache usage totals copied from the SDK's result event onto the final assistant bubble. */
  cacheReadInputTokens?: number | null
  cacheCreationInputTokens?: number | null
  /** Model that generated this message. Null for user/tool/system, or if the SDK didn't report it. */
  model: string | null
  createdAt: string
  toolEventIds: string[]
}

export interface SdkToolEventView {
  id: string
  messageId: string | null
  toolName: string
  input: Record<string, unknown>
  result: string | null
  isError: boolean
  durationMs: number | null
  status: 'running' | 'done' | 'error'
}

interface RestoredToolEvent {
  id: string
  messageId: string
  parentSubagentId: string | null
  toolName: string
  input: Record<string, unknown>
  resultText: string | null
  isError: boolean
  durationMs: number | null
  answers?: Record<string, SdkAskUserQuestionAnswer>
}

export type SdkSessionStatus = 'idle' | 'streaming' | 'ended' | 'error' | 'cancelled' | 'starting'

export interface SdkSubagentView {
  /** Subagent id — equals the parent Task/Agent tool_use_id. */
  id: string
  task: string
  agentType: string
  status: 'running' | 'success' | 'error'
  messages: SdkMessageView[]
  toolEvents: Record<string, SdkToolEventView>
}

export interface SdkSessionState {
  conversationId: string
  conversation: SdkConversation | null
  messages: SdkMessageView[]
  toolEvents: Record<string, SdkToolEventView>
  /** Keyed by the parent Task/Agent tool_use_id. */
  subagents: Record<string, SdkSubagentView>
  pendingAttention: AttentionBlock[]
  sdkSessionId: string | null
  status: SdkSessionStatus
  tokensIn: number
  tokensOut: number
  costUsd: number
  /**
   * Per-turn snapshot from the most recent `usage` event. `input_tokens +
   * cache_read + cache_creation` equals what the model reasoned over for
   * that turn, which approximates the live context-window occupancy.
   */
  lastInputTokens: number | null
  lastCacheReadTokens: number | null
  lastCacheCreationTokens: number | null
  /** Model's total context window in tokens (from models.dev `limit.context`). */
  contextWindow: number | null
  lastError: string | null
  /** Epoch ms captured when the user sent the most recent message; null once the first assistant.message of that turn consumes it. */
  lastUserSendAt: number | null
}

const EMPTY_USAGE = { tokensIn: 0, tokensOut: 0, costUsd: 0 }

export const sdkSessions: Record<string, SdkSessionState> = $state({})
// This is lifecycle bookkeeping, not UI state. Keeping it non-reactive prevents
// pane effects from depending on the same value they mutate.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const subscriptions = new Map<string, () => void>()

function ensure(id: string): SdkSessionState {
  const existing = sdkSessions[id]
  if (existing) return existing
  sdkSessions[id] = {
    conversationId: id,
    conversation: null,
    messages: [],
    toolEvents: {},
    subagents: {},
    pendingAttention: [],
    sdkSessionId: null,
    status: 'idle',
    ...EMPTY_USAGE,
    lastInputTokens: null,
    lastCacheReadTokens: null,
    lastCacheCreationTokens: null,
    contextWindow: null,
    lastError: null,
    lastUserSendAt: null,
  }
  return sdkSessions[id]
}

export function getSession(id: string): SdkSessionState | undefined {
  return sdkSessions[id]
}

/**
 * Hydrate transcript from DB and start listening for live events. Safe to
 * call on an already-open conversation — it is a no-op if a listener is
 * already attached.
 */
export async function openConversation(id: string): Promise<SdkSessionState> {
  const state = ensure(id)

  // Attach the IPC listener synchronously BEFORE awaiting the transcript.
  // Without this, two rapid openConversation(id) calls both pass the guard
  // during the await window, each call ends with its own subscribe(), and
  // the second overwrites the unsubscribe handle — leaking the first listener.
  // Symptom: MaxListenersExceededWarning after a few pane toggles, then
  // every event fires N reducers → the renderer gets buried.
  if (!subscriptions.has(id)) {
    subscriptions.set(
      id,
      window.api.sdkAgent.subscribe(id, (event) => reduce(state, event)),
    )
  }

  const transcript = await window.api.sdkAgent.getTranscript(id)
  const messageRecords = transcript.messages ?? []
  const toolEventRecords = transcript.toolEvents ?? []
  const rootMessageRecords = messageRecords.filter((message) => !message.parentSubagentId)
  const nestedMessageRecords = messageRecords.filter((message) => !!message.parentSubagentId)
  const rootToolEventRecords = toolEventRecords.filter((event) => !event.parentSubagentId)
  const nestedToolEventRecords = toolEventRecords.filter((event) => !!event.parentSubagentId)
  const messages = rootMessageRecords.map(messageFromRecord)
  const restoredToolEvents = restoredToolEventsFromTranscript(messages, rootToolEventRecords)
  state.conversation = transcript.conversation ?? null
  state.messages = attachToolEventsToMessages(messages, restoredToolEvents)
  state.toolEvents = Object.fromEntries(
    restoredToolEvents.map((event) => [event.id, toolEventFromRecord(event)]),
  )
  state.subagents = restoredSubagentsFromTranscript(
    nestedMessageRecords,
    nestedToolEventRecords,
    restoredToolEvents,
  )
  state.pendingAttention = mergeAttentionBlocks(
    restoredQuestionBlocksFromToolEvents(restoredToolEvents),
    state.pendingAttention,
  )
  state.sdkSessionId = transcript.conversation?.sdkSessionId ?? null
  return state
}

export function closeConversation(id: string): void {
  subscriptions.get(id)?.()
  subscriptions.delete(id)
}

export function destroySession(id: string): void {
  closeConversation(id)
  delete sdkSessions[id]
}

// --- Actions (passthrough to main) ---

export async function sendMessage(
  id: string,
  text: string,
  options: {
    attachments?: SdkAttachment[]
    modelOverride?: string
    permissionModeOverride?: SdkPermissionMode
    effortLevelOverride?: SdkEffortLevel | null
  } = {},
): Promise<{ ok: true } | { error: string }> {
  const state = ensure(id)
  state.status = 'streaming'
  state.lastError = null
  state.lastUserSendAt = Date.now()
  // Clear the previous turn's per-step snapshot so the fallback in the
  // `usage` reducer (for short turns that skip assistant.message) kicks in.
  state.lastInputTokens = null
  state.lastCacheReadTokens = null
  state.lastCacheCreationTokens = null
  // Optimistic: push the user message; the backend event will not emit a user
  // echo, so this is authoritative.
  state.messages = [
    ...state.messages,
    {
      id: `local-${Date.now()}`,
      role: 'user',
      content: text,
      thinking: '',
      contentBlocks: buildOptimisticUserContent(text, options.attachments),
      tokensIn: null,
      tokensOut: null,
      costUsd: null,
      elapsedMs: null,
      model: null,
      createdAt: nowIso(),
      toolEventIds: [],
    },
  ]
  let result: { ok: true } | { error: string }
  try {
    result = await window.api.sdkAgent.send({
      conversationId: id,
      text,
      attachments: options.attachments?.map(serializeAttachment),
      modelOverride: options.modelOverride,
      permissionModeOverride: options.permissionModeOverride,
      effortLevelOverride: options.effortLevelOverride,
    })
  } catch (e) {
    result = { error: e instanceof Error ? e.message : String(e) }
  }
  if ('error' in result) {
    state.status = 'error'
    state.lastError = result.error
  }
  return result
}

export async function cancel(id: string): Promise<void> {
  await window.api.sdkAgent.cancel(id)
}

export async function setModel(id: string, model: string): Promise<void> {
  const state = sdkSessions[id]
  if (state?.conversation) state.conversation = { ...state.conversation, model }
  await window.api.sdkAgent.updateConversation({ conversationId: id, model })
}

export async function setPermissionMode(id: string, mode: SdkPermissionMode): Promise<void> {
  const state = sdkSessions[id]
  if (state?.conversation) state.conversation = { ...state.conversation, permissionMode: mode }
  await window.api.sdkAgent.updateConversation({ conversationId: id, permissionMode: mode })
}

export function setContextWindow(id: string, contextWindow: number | null): void {
  const state = sdkSessions[id]
  if (!state) return
  state.contextWindow = contextWindow
}

/**
 * Approximate percentage of the model's context window consumed by the most
 * recent turn. Returns `null` when either the window size or the turn's token
 * counts are unknown — e.g. before the first `usage` event lands or when the
 * model isn't in models.dev's catalog.
 */
export function contextPercentFor(state: SdkSessionState | undefined): number | null {
  if (!state || !state.contextWindow) return null
  const used =
    (state.lastInputTokens ?? 0) +
    (state.lastCacheReadTokens ?? 0) +
    (state.lastCacheCreationTokens ?? 0)
  if (used <= 0) return null
  return Math.round((used / state.contextWindow) * 100)
}

export function contextTokensUsedFor(state: SdkSessionState | undefined): number {
  if (!state) return 0
  return (
    (state.lastInputTokens ?? 0) +
    (state.lastCacheReadTokens ?? 0) +
    (state.lastCacheCreationTokens ?? 0)
  )
}

export async function setEffortLevel(id: string, effort: SdkEffortLevel | null): Promise<void> {
  const state = sdkSessions[id]
  if (state?.conversation) {
    state.conversation = { ...state.conversation, effortLevel: effort }
  }
  await window.api.sdkAgent.updateConversation({ conversationId: id, effortLevel: effort })
}

export async function respondPermission(
  id: string,
  requestId: string,
  decision: SdkToolDecision,
): Promise<void> {
  const state = sdkSessions[id]
  if (state) markAttentionResolved(state, requestId, decision === 'deny' ? 'denied' : 'granted')
  await window.api.sdkAgent.respondPermission({ conversationId: id, requestId, decision })
}

export async function respondQuestion(
  id: string,
  requestId: string,
  answers: Record<string, SdkAskUserQuestionAnswer>,
): Promise<void> {
  const state = sdkSessions[id]
  const plainAnswers = serializeQuestionAnswers(answers)
  try {
    await window.api.sdkAgent.respondQuestion({
      conversationId: id,
      requestId,
      answers: plainAnswers,
    })
    if (state) markAttentionResolved(state, requestId, 'resolved', undefined, plainAnswers)
  } catch (e) {
    console.error('[sdk-agent][question] failed to submit answers', {
      id,
      requestId,
      error: e instanceof Error ? e.message : String(e),
    })
  }
}

export async function respondPlan(
  id: string,
  requestId: string,
  decision: SdkPlanDecision,
): Promise<void> {
  const state = sdkSessions[id]
  if (state) {
    if (decision.action === 'approve' && state.conversation) {
      state.conversation = {
        ...state.conversation,
        permissionMode: decision.permissionMode,
      }
    }
    markAttentionResolved(
      state,
      requestId,
      decision.action === 'approve' ? 'approved' : 'rejected',
      decision.action === 'reject' ? decision.feedback : undefined,
      undefined,
      decision.action === 'approve' ? decision.permissionMode : undefined,
    )
  }
  await window.api.sdkAgent.respondPlan({ conversationId: id, requestId, decision })
}

// --- Event reducer ---

function reduce(state: SdkSessionState, event: SdkAgentEvent): void {
  match(event)
    .with({ _tag: 'session.init' }, (e) => {
      state.sdkSessionId = e.sdkSessionId as string
      state.status = 'streaming'
    })
    .with({ _tag: 'assistant.delta' }, (e) => {
      // Append to current streaming assistant message; create if missing.
      const { message: last, changed } = ensureStreamingAssistantMessage(
        state.messages,
        e.messageId as string,
      )
      if (changed) {
        // Stamp wall-clock elapsed the moment the streaming bubble appears,
        // not only when the SDK emits a final `assistant.message`. Short
        // turns skip that event and only fire `stream_event` + `result`, so
        // without this the footer's `groupHasFooter` gate would be false
        // until the `usage` event lands — and sometimes not at all.
        if (last.elapsedMs === null && state.lastUserSendAt !== null) {
          last.elapsedMs = Date.now() - state.lastUserSendAt
          state.lastUserSendAt = null
        }
        state.messages = [...state.messages]
      }
      last.content = last.content + e.text
      const first = last.contentBlocks[0] as { type: string; text?: string }
      if (first && first.type === 'text') first.text = last.content
    })
    .with({ _tag: 'assistant.thinking' }, (e) => {
      const { message: last, changed } = ensureStreamingAssistantMessage(
        state.messages,
        e.messageId as string,
      )
      if (changed) {
        if (last.elapsedMs === null && state.lastUserSendAt !== null) {
          last.elapsedMs = Date.now() - state.lastUserSendAt
          state.lastUserSendAt = null
        }
        state.messages = [...state.messages]
      }
      last.thinking = last.thinking + e.text
    })
    .with({ _tag: 'assistant.message' }, (e) => {
      // Per-step usage snapshot is the correct source for "live" context-
      // window occupancy. The `result` event's totals are cumulative across
      // every step of the multi-step query and routinely exceed the window.
      if (typeof e.tokensIn === 'number') {
        state.lastInputTokens = e.tokensIn
        state.lastCacheReadTokens = e.cacheReadInputTokens ?? null
        state.lastCacheCreationTokens = e.cacheCreationInputTokens ?? null
      }
      const idx = state.messages.findIndex(
        (m) => m.role === 'assistant' && m.id === (e.messageId as string),
      )
      // Consume lastUserSendAt on the first assistant message of the turn.
      const priorElapsed = idx >= 0 ? state.messages[idx].elapsedMs : null
      let elapsedMs: number | null = priorElapsed
      if (elapsedMs === null && state.lastUserSendAt !== null) {
        elapsedMs = Date.now() - state.lastUserSendAt
        state.lastUserSendAt = null
      }
      const priorModel = idx >= 0 ? state.messages[idx].model : null
      const priorThinking = idx >= 0 ? state.messages[idx].thinking : ''
      const priorContentBlocks = idx >= 0 ? state.messages[idx].contentBlocks : undefined
      const mergedContentBlocks = mergeAssistantContentBlocks(
        priorContentBlocks,
        e.content as Array<Record<string, unknown>>,
      )
      const finalized: SdkMessageView = {
        id: e.messageId as string,
        role: 'assistant',
        content: blocksToText(mergedContentBlocks),
        thinking: priorThinking,
        contentBlocks: mergedContentBlocks,
        tokensIn: e.tokensIn ?? null,
        tokensOut: e.tokensOut ?? null,
        costUsd: e.costUsd ?? (idx >= 0 ? state.messages[idx].costUsd : null),
        elapsedMs,
        model: e.model ?? priorModel,
        createdAt: nowIso(),
        toolEventIds: idx >= 0 ? state.messages[idx].toolEventIds : [],
      }
      if (idx >= 0) state.messages[idx] = finalized
      else state.messages = [...state.messages, finalized]
    })
    .with({ _tag: 'tool.start' }, (e) => {
      state.toolEvents[e.toolEventId] = {
        id: e.toolEventId,
        messageId: e.messageId as string,
        toolName: e.name,
        input: e.input,
        result: null,
        isError: false,
        durationMs: null,
        status: 'running',
      }
      const message = state.messages.find((m) => m.id === (e.messageId as string))
      if (
        message &&
        message.role === 'assistant' &&
        !message.toolEventIds.includes(e.toolEventId)
      ) {
        message.toolEventIds = [...message.toolEventIds, e.toolEventId]
      }
    })
    .with({ _tag: 'tool.result' }, (e) => {
      const ev = state.toolEvents[e.toolEventId]
      if (!ev) return
      ev.result = e.result
      ev.isError = e.isError
      ev.durationMs = e.durationMs
      ev.status = e.isError ? 'error' : 'done'
    })
    .with({ _tag: 'tool.permission_request' }, (e) => {
      state.pendingAttention = [
        ...state.pendingAttention,
        {
          kind: 'permission',
          requestId: e.requestId,
          messageId: lastAssistantId(state),
          toolName: e.toolName,
          input: e.input,
          status: 'waiting',
        },
      ]
    })
    .with({ _tag: 'ask_user_question' }, (e) => {
      state.pendingAttention = [
        ...state.pendingAttention,
        {
          kind: 'question',
          requestId: e.requestId,
          messageId: lastAssistantId(state),
          questions: e.questions as never,
          status: 'waiting',
        },
      ]
    })
    .with({ _tag: 'plan_mode_exit' }, (e) => {
      state.pendingAttention = [
        ...state.pendingAttention,
        {
          kind: 'plan',
          requestId: e.requestId,
          messageId: lastAssistantId(state),
          toolEventId: e.toolEventId,
          plan: e.plan,
          allowedPrompts: e.allowedPrompts ?? [],
          status: 'waiting',
        },
      ]
    })
    .with({ _tag: 'usage' }, (e) => {
      state.tokensIn += e.inputTokens
      state.tokensOut += e.outputTokens
      if (typeof e.costUsd === 'number') state.costUsd += e.costUsd
      // Only fall back to the cumulative `result.usage` for context-window
      // estimation when no per-step snapshot has been recorded yet (i.e. a
      // short turn that skipped the `type: 'assistant'` event entirely).
      // Multi-step queries already set these from each per-step
      // `assistant.message`, which is the authoritative per-turn snapshot.
      if (state.lastInputTokens === null) {
        state.lastInputTokens = e.inputTokens
        state.lastCacheReadTokens = e.cacheReadInputTokens ?? null
        state.lastCacheCreationTokens = e.cacheCreationInputTokens ?? null
      }
      // Attach the turn's totals to the final assistant message so the
      // renderer's MessageMeta footer can surface duration/cost/tokens
      // from the SDK's result summary instead of just per-chunk deltas.
      let target = -1
      for (let i = state.messages.length - 1; i >= 0; i--) {
        if (state.messages[i].role === 'assistant') {
          target = i
          break
        }
      }
      if (target < 0) {
        // No assistant message to attach to — synthesize a minimal one so
        // the footer still renders with the server totals.
        state.messages = [
          ...state.messages,
          {
            id: `usage-${Date.now()}`,
            role: 'assistant',
            content: '',
            thinking: '',
            contentBlocks: [{ type: 'text', text: '' }],
            tokensIn: e.inputTokens,
            tokensOut: e.outputTokens,
            costUsd: e.costUsd ?? null,
            elapsedMs: e.durationMs ?? null,
            cacheReadInputTokens: e.cacheReadInputTokens ?? null,
            cacheCreationInputTokens: e.cacheCreationInputTokens ?? null,
            model: null,
            createdAt: nowIso(),
            toolEventIds: [],
          },
        ]
        return
      }
      const m = state.messages[target]
      const next: SdkMessageView = {
        ...m,
        tokensIn: e.inputTokens,
        tokensOut: e.outputTokens,
        costUsd: typeof e.costUsd === 'number' ? e.costUsd : m.costUsd,
        elapsedMs: typeof e.durationMs === 'number' ? e.durationMs : m.elapsedMs,
        cacheReadInputTokens: e.cacheReadInputTokens ?? m.cacheReadInputTokens ?? null,
        cacheCreationInputTokens: e.cacheCreationInputTokens ?? m.cacheCreationInputTokens ?? null,
      }
      // Replace the whole array in one shot — the safer signal to Svelte's
      // $state proxy than mutating an index in-place, which in some
      // reactive flows doesn't propagate to derived readers.
      state.messages = state.messages.map((msg, idx) => (idx === target ? next : msg))
    })
    .with({ _tag: 'error' }, (e) => {
      state.status = 'error'
      state.lastError = ((e.error as { _tag?: string })._tag ?? 'unknown') as string
    })
    .with({ _tag: 'session.end' }, (e) => {
      state.status = match(e.reason)
        .with('completed', () => 'ended' as const)
        .with('aborted', () => 'cancelled' as const)
        .with('error', () => 'error' as const)
        .exhaustive()
      // Only a fresh error event should repopulate lastError; a clean
      // completion / cancel clears the previous session's error marker.
      if (e.reason !== 'error') state.lastError = null
    })
    .with({ _tag: 'subagent.start' }, (e) => {
      if (state.subagents[e.subagentId]) return
      state.subagents[e.subagentId] = {
        id: e.subagentId,
        task: e.task,
        agentType: e.agentType,
        status: 'running',
        messages: [],
        toolEvents: {},
      }
    })
    .with({ _tag: 'subagent.event' }, (e) => {
      const subagent = state.subagents[e.subagentId]
      if (!subagent) return
      reduceSubagent(subagent, e.event)
    })
    .with({ _tag: 'subagent.end' }, (e) => {
      const subagent = state.subagents[e.subagentId]
      if (!subagent) return
      subagent.status = e.status === 'success' ? 'success' : 'error'
    })
    .exhaustive()
}

/**
 * Apply a nested event against a single subagent's scoped message/tool state.
 * Mirrors the subset of `reduce` that is meaningful inside a Task-tool run.
 */
function reduceSubagent(subagent: SdkSubagentView, event: SdkAgentEvent): void {
  match(event)
    .with({ _tag: 'assistant.delta' }, (e) => {
      const { message: last, changed } = ensureStreamingAssistantMessage(
        subagent.messages,
        e.messageId as string,
      )
      if (changed) subagent.messages = [...subagent.messages]
      last.content = last.content + e.text
      const first = last.contentBlocks[0] as { type: string; text?: string }
      if (first && first.type === 'text') first.text = last.content
    })
    .with({ _tag: 'assistant.thinking' }, (e) => {
      const { message: last, changed } = ensureStreamingAssistantMessage(
        subagent.messages,
        e.messageId as string,
      )
      if (changed) subagent.messages = [...subagent.messages]
      last.thinking = last.thinking + e.text
    })
    .with({ _tag: 'assistant.message' }, (e) => {
      const idx = subagent.messages.findIndex(
        (m) => m.role === 'assistant' && m.id === (e.messageId as string),
      )
      const priorModel = idx >= 0 ? subagent.messages[idx].model : null
      const priorThinking = idx >= 0 ? subagent.messages[idx].thinking : ''
      const priorContentBlocks = idx >= 0 ? subagent.messages[idx].contentBlocks : undefined
      const mergedContentBlocks = mergeAssistantContentBlocks(
        priorContentBlocks,
        e.content as Array<Record<string, unknown>>,
      )
      const finalized: SdkMessageView = {
        id: e.messageId as string,
        role: 'assistant',
        content: blocksToText(mergedContentBlocks),
        thinking: priorThinking,
        contentBlocks: mergedContentBlocks,
        tokensIn: e.tokensIn ?? null,
        tokensOut: e.tokensOut ?? null,
        costUsd: e.costUsd ?? (idx >= 0 ? subagent.messages[idx].costUsd : null),
        elapsedMs: null,
        model: e.model ?? priorModel,
        createdAt: nowIso(),
        toolEventIds: idx >= 0 ? subagent.messages[idx].toolEventIds : [],
      }
      if (idx >= 0) subagent.messages[idx] = finalized
      else subagent.messages = [...subagent.messages, finalized]
    })
    .with({ _tag: 'tool.start' }, (e) => {
      subagent.toolEvents[e.toolEventId] = {
        id: e.toolEventId,
        messageId: e.messageId as string,
        toolName: e.name,
        input: e.input,
        result: null,
        isError: false,
        durationMs: null,
        status: 'running',
      }
      const message = subagent.messages.find((m) => m.id === (e.messageId as string))
      if (
        message &&
        message.role === 'assistant' &&
        !message.toolEventIds.includes(e.toolEventId)
      ) {
        message.toolEventIds = [...message.toolEventIds, e.toolEventId]
      }
    })
    .with({ _tag: 'tool.result' }, (e) => {
      const ev = subagent.toolEvents[e.toolEventId]
      if (!ev) return
      ev.result = e.result
      ev.isError = e.isError
      ev.durationMs = e.durationMs
      ev.status = e.isError ? 'error' : 'done'
    })
    .otherwise(() => {
      // Subagents can emit other events (permission_request, usage, etc.)
      // we don't render yet. Silently ignore — they already went through the
      // provider's aggregator, and not forwarding them here avoids them
      // leaking into the parent conversation state.
    })
}

function ensureStreamingAssistantMessage(
  messages: SdkMessageView[],
  messageId: string,
): { message: SdkMessageView; changed: boolean } {
  let last = messages[messages.length - 1]
  if (last?.role === 'assistant' && last.id === messageId) {
    return { message: last, changed: false }
  }

  last = {
    id: messageId,
    role: 'assistant',
    content: '',
    thinking: '',
    contentBlocks: [{ type: 'text', text: '' }],
    tokensIn: null,
    tokensOut: null,
    costUsd: null,
    elapsedMs: null,
    model: null,
    createdAt: nowIso(),
    toolEventIds: [],
  }
  messages.push(last)
  return { message: last, changed: true }
}

function messageFromRecord(record: SdkMessageRecord): SdkMessageView {
  const contentBlocks = record.contentBlocks as Array<Record<string, unknown>>
  return {
    id: record.id as string,
    role: record.role,
    // Regenerate display text from content blocks (text-only). The persisted
    // `record.content` is FTS-indexed plain text that historically included
    // tool-call placeholders like `[tool:Bash]`; deriving from blocks here
    // keeps old rows from leaking those into the bubble.
    content: blocksToText(contentBlocks as ReadonlyArray<{ type: string; [k: string]: unknown }>),
    thinking: record.thinking,
    contentBlocks,
    tokensIn: record.tokensIn,
    tokensOut: record.tokensOut,
    costUsd: null,
    elapsedMs: null,
    model: record.model,
    createdAt: record.createdAt,
    toolEventIds: [],
  }
}

function toolEventFromRecord(record: RestoredToolEvent): SdkToolEventView {
  return {
    id: record.id,
    messageId: record.messageId,
    toolName: record.toolName,
    input: record.input,
    result: record.resultText,
    isError: record.isError,
    durationMs: record.durationMs,
    status: toolEventStatus(record),
  }
}

function restoredSubagentsFromTranscript(
  messageRecords: SdkMessageRecord[],
  toolEventRecords: SdkToolEventRecord[],
  rootToolEvents: RestoredToolEvent[],
): Record<string, SdkSubagentView> {
  const metaById: Record<string, Pick<SdkSubagentView, 'task' | 'agentType' | 'status'>> = {}
  for (const event of rootToolEvents) {
    if (!isSubagentRootTool(event.toolName)) continue
    metaById[event.id] = {
      task: extractSubagentTask(event.input),
      agentType: extractSubagentType(event.input),
      status: event.isError ? 'error' : event.resultText !== null ? 'success' : 'running',
    }
  }

  const messagesByParent: Record<string, SdkMessageView[]> = {}
  for (const record of messageRecords) {
    if (!record.parentSubagentId) continue
    messagesByParent[record.parentSubagentId] = [
      ...(messagesByParent[record.parentSubagentId] ?? []),
      messageFromRecord(record),
    ]
  }

  const toolEventsByParent: Record<string, SdkToolEventRecord[]> = {}
  for (const record of toolEventRecords) {
    if (!record.parentSubagentId) continue
    toolEventsByParent[record.parentSubagentId] = [
      ...(toolEventsByParent[record.parentSubagentId] ?? []),
      record,
    ]
  }

  const ids = new Set([
    ...Object.keys(metaById),
    ...Object.keys(messagesByParent),
    ...Object.keys(toolEventsByParent),
  ])

  const subagents: Record<string, SdkSubagentView> = {}
  for (const id of ids) {
    const messages = messagesByParent[id] ?? []
    const restoredToolEvents = restoredToolEventsFromTranscript(
      messages,
      toolEventsByParent[id] ?? [],
    )
    subagents[id] = {
      id,
      task: metaById[id]?.task ?? 'Sub-agent task',
      agentType: metaById[id]?.agentType ?? 'general',
      status: metaById[id]?.status ?? 'running',
      messages: attachToolEventsToMessages(messages, restoredToolEvents),
      toolEvents: Object.fromEntries(
        restoredToolEvents.map((event) => [event.id, toolEventFromRecord(event)]),
      ),
    }
  }
  return subagents
}

function isSubagentRootTool(toolName: string): boolean {
  return toolName === 'Task' || toolName === 'Agent'
}

function extractSubagentTask(input: Record<string, unknown>): string {
  if (typeof input.description === 'string' && input.description.trim()) return input.description
  if (typeof input.prompt === 'string' && input.prompt.trim()) {
    return input.prompt.length > 120 ? input.prompt.slice(0, 120) + '…' : input.prompt
  }
  return 'Sub-agent task'
}

function extractSubagentType(input: Record<string, unknown>): string {
  if (typeof input.subagent_type === 'string' && input.subagent_type.trim()) {
    return input.subagent_type
  }
  return 'general'
}

function toolEventStatus(record: RestoredToolEvent): SdkToolEventView['status'] {
  if (record.isError) return 'error'
  if (record.resultText !== null || record.durationMs !== null) return 'done'
  if (record.toolName === 'AskUserQuestion') return 'done'
  return 'running'
}

function restoredToolEventsFromTranscript(
  messages: SdkMessageView[],
  records: SdkToolEventRecord[],
): RestoredToolEvent[] {
  const byId: Record<string, RestoredToolEvent> = {}
  const seenInContent: Record<string, true> = {}
  for (const record of records) {
    byId[record.id] = {
      id: record.id,
      messageId: record.messageId as string,
      parentSubagentId: record.parentSubagentId,
      toolName: record.toolName,
      input: record.input,
      resultText: record.resultText,
      isError: record.isError,
      durationMs: record.durationMs,
      ...(record.answers ? { answers: record.answers } : {}),
    }
  }

  const ordered: RestoredToolEvent[] = []
  for (const message of messages) {
    for (const block of message.contentBlocks) {
      if (!isToolUseContentBlock(block)) continue
      const existing = byId[block.id]
      const restored: RestoredToolEvent = existing
        ? {
            ...existing,
            messageId: message.id,
            toolName: block.name,
          }
        : {
            id: block.id,
            messageId: message.id,
            parentSubagentId: null,
            toolName: block.name,
            input: block.input,
            resultText: null,
            isError: false,
            durationMs: 0,
          }
      byId[block.id] = restored
      seenInContent[block.id] = true
      ordered.push(restored)
    }
  }

  for (const event of Object.values(byId)) {
    if (seenInContent[event.id]) continue
    ordered.push(event)
  }
  return ordered
}

function isToolUseContentBlock(
  block: Record<string, unknown>,
): block is { type: 'tool_use'; id: string; name: string; input: Record<string, unknown> } {
  return (
    block.type === 'tool_use' &&
    typeof block.id === 'string' &&
    typeof block.name === 'string' &&
    !!block.input &&
    typeof block.input === 'object' &&
    !Array.isArray(block.input)
  )
}

function attachToolEventsToMessages(
  messages: SdkMessageView[],
  records: RestoredToolEvent[],
): SdkMessageView[] {
  const byMessage: Record<string, string[]> = {}
  for (const record of records) {
    byMessage[record.messageId] = [...(byMessage[record.messageId] ?? []), record.id]
  }
  return messages.map((message) => ({
    ...message,
    toolEventIds: byMessage[message.id] ?? [],
  }))
}

function restoredQuestionBlocksFromToolEvents(records: RestoredToolEvent[]): AttentionBlock[] {
  return records.flatMap((record) => {
    if (record.toolName !== 'AskUserQuestion') return []
    const questions = questionsFromToolInput(record.input)
    if (!questions) return []
    const answers = record.answers ?? questionAnswersFromToolInput(record.input, questions)
    return [
      {
        kind: 'question' as const,
        requestId: `restored-question-${record.id}`,
        messageId: record.messageId,
        questions,
        status: 'resolved' as const,
        answers,
      },
    ]
  })
}

function mergeAttentionBlocks(
  restored: AttentionBlock[],
  existing: AttentionBlock[],
): AttentionBlock[] {
  const merged = [...restored]
  for (const block of existing) {
    const duplicateIndex = merged.findIndex((candidate) => sameAttentionBlock(candidate, block))
    if (duplicateIndex >= 0) {
      merged[duplicateIndex] = block
      continue
    }
    merged.push(block)
  }
  return merged
}

function sameAttentionBlock(a: AttentionBlock, b: AttentionBlock): boolean {
  if (a.kind !== b.kind || a.messageId !== b.messageId) return false
  if (a.kind === 'question' && b.kind === 'question') {
    return JSON.stringify(a.questions) === JSON.stringify(b.questions)
  }
  if (a.kind === 'plan' && b.kind === 'plan') return a.plan === b.plan
  if (a.kind === 'permission' && b.kind === 'permission') {
    return a.toolName === b.toolName && JSON.stringify(a.input) === JSON.stringify(b.input)
  }
  return false
}

function questionsFromToolInput(
  input: Record<string, unknown>,
): QuestionAttentionBlock['questions'] | null {
  const questions = input.questions
  if (!Array.isArray(questions) || !questions.every(isQuestionLike)) return null
  return questions as QuestionAttentionBlock['questions']
}

function isQuestionLike(value: unknown): boolean {
  if (!value || typeof value !== 'object') return false
  const question = value as Record<string, unknown>
  return (
    typeof question.header === 'string' &&
    typeof question.question === 'string' &&
    Array.isArray(question.options) &&
    question.options.every(isQuestionOptionLike)
  )
}

function isQuestionOptionLike(value: unknown): boolean {
  if (!value || typeof value !== 'object') return false
  const option = value as Record<string, unknown>
  return typeof option.label === 'string'
}

function questionAnswersFromToolInput(
  input: Record<string, unknown>,
  questions: QuestionAttentionBlock['questions'],
): Record<string, SdkAskUserQuestionAnswer> | undefined {
  const rawAnswers = input.answers
  if (!rawAnswers || typeof rawAnswers !== 'object') return undefined
  const rawAnnotations =
    input.annotations && typeof input.annotations === 'object'
      ? (input.annotations as Record<string, unknown>)
      : {}
  const answers: Record<string, SdkAskUserQuestionAnswer> = {}
  for (const question of questions) {
    const value = (rawAnswers as Record<string, unknown>)[question.question]
    const answer = answerFromToolValue(question, value)
    if (!answer) continue
    const annotation = rawAnnotations[question.question]
    if (annotation && typeof annotation === 'object') {
      const notes = (annotation as Record<string, unknown>).notes
      if (typeof notes === 'string' && notes.trim().length > 0) answer.notes = notes
    }
    answers[question.question] = answer
  }
  return Object.keys(answers).length > 0 ? answers : undefined
}

function answerFromToolValue(
  question: QuestionSpec,
  value: unknown,
): SdkAskUserQuestionAnswer | null {
  if (
    value &&
    typeof value === 'object' &&
    Array.isArray((value as { selected?: unknown }).selected)
  ) {
    const answer = value as { selected: unknown[]; other?: unknown; notes?: unknown }
    return {
      selected: answer.selected.filter((label): label is string => typeof label === 'string'),
      ...(typeof answer.other === 'string' && answer.other.length > 0
        ? { other: answer.other }
        : {}),
      ...(typeof answer.notes === 'string' && answer.notes.length > 0
        ? { notes: answer.notes }
        : {}),
    }
  }
  if (typeof value !== 'string' || value.trim().length === 0) return null
  const optionLabels = new Set(question.options.map((option) => option.label))
  const selected: string[] = []
  const other: string[] = []
  for (const part of value
    .split(',')
    .map((label) => label.trim())
    .filter(Boolean)) {
    if (optionLabels.has(part)) selected.push(part)
    else other.push(part)
  }
  return {
    selected: other.length > 0 ? [...selected, 'Other'] : selected,
    ...(other.length > 0 ? { other: other.join(', ') } : {}),
  }
}

function nowIso(): string {
  return new Date().toISOString()
}

function blocksToText(blocks: readonly { type: string; [k: string]: unknown }[]): string {
  const out: string[] = []
  for (const b of blocks) {
    if (b.type === 'text' && typeof b.text === 'string') out.push(b.text)
  }
  return out.join('\n\n')
}

function mergeAssistantContentBlocks(
  previous: readonly Record<string, unknown>[] | undefined,
  next: readonly Record<string, unknown>[],
): Array<Record<string, unknown>> {
  const nextHasText = next.some(
    (block) => block.type === 'text' && typeof block.text === 'string' && block.text.length > 0,
  )
  if (nextHasText || !previous?.length) return [...next]

  const previousTextBlocks = previous.filter(
    (block) => block.type === 'text' && typeof block.text === 'string' && block.text.length > 0,
  )
  if (previousTextBlocks.length === 0) return [...next]

  return [...previousTextBlocks, ...next]
}

function lastAssistantId(state: SdkSessionState): string | null {
  for (let i = state.messages.length - 1; i >= 0; i--) {
    if (state.messages[i].role === 'assistant') return state.messages[i].id
  }
  return null
}

function markAttentionResolved(
  state: SdkSessionState,
  requestId: string,
  status: 'resolved' | 'granted' | 'denied' | 'approved' | 'rejected',
  feedback?: string,
  answers?: Record<string, SdkAskUserQuestionAnswer>,
  permissionMode?: Exclude<SdkPermissionMode, 'plan'>,
): void {
  state.pendingAttention = state.pendingAttention.map((b) => {
    if (b.requestId !== requestId) return b
    return match(b)
      .with({ kind: 'question' }, (q) => ({ ...q, status: 'resolved' as const, answers }))
      .with({ kind: 'plan' }, (p) => ({
        ...p,
        status: (status === 'approved' ? 'approved' : 'rejected') as 'approved' | 'rejected',
        feedback,
        permissionMode,
      }))
      .with({ kind: 'permission' }, (p) => ({
        ...p,
        status: (status === 'granted' ? 'granted' : 'denied') as 'granted' | 'denied',
      }))
      .exhaustive()
  })
}

function serializeQuestionAnswers(
  answers: Record<string, SdkAskUserQuestionAnswer>,
): Record<string, SdkAskUserQuestionAnswer> {
  const plain: Record<string, SdkAskUserQuestionAnswer> = {}
  for (const [question, answer] of Object.entries(answers)) {
    plain[question] = {
      selected: Array.isArray(answer.selected) ? [...answer.selected] : [],
      ...(typeof answer.other === 'string' && answer.other.length > 0
        ? { other: answer.other }
        : {}),
      ...(typeof answer.notes === 'string' && answer.notes.length > 0
        ? { notes: answer.notes }
        : {}),
    }
  }
  return plain
}

function buildOptimisticUserContent(
  text: string,
  attachments: SdkAttachment[] | undefined,
): Array<Record<string, unknown>> {
  const blocks: Array<Record<string, unknown>> = []
  if (text.length > 0) blocks.push({ type: 'text', text })
  for (const attachment of attachments ?? []) {
    const preview = (attachment as SdkAttachment & { previewDataUrl?: string }).previewDataUrl
    if (attachment.kind === 'image' && preview) {
      const image = imageBlockFromDataUrl(preview, attachment.filename)
      if (image) {
        blocks.push(image)
        continue
      }
    }
    blocks.push({
      type: 'text',
      text: `[attachment:${attachment.filename}]`,
    })
  }
  return blocks.length > 0 ? blocks : [{ type: 'text', text: '' }]
}

function imageBlockFromDataUrl(dataUrl: string, filename: string): Record<string, unknown> | null {
  const match = /^data:([^;,]+);base64,(.*)$/.exec(dataUrl)
  if (!match) return null
  return {
    type: 'image',
    source: {
      type: 'base64',
      media_type: match[1],
      data: match[2],
    },
    filename,
  }
}

function serializeAttachment(attachment: SdkAttachment): SdkAttachment {
  return {
    id: attachment.id,
    kind: attachment.kind,
    filename: attachment.filename,
    path: attachment.path,
    mimeType: attachment.mimeType,
    sizeBytes: attachment.sizeBytes,
  }
}
