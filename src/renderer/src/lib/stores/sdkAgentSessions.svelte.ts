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
  const toolEventRecords = transcript.toolEvents ?? []
  const messages = transcript.messages.map(messageFromRecord)
  const restoredToolEvents = restoredToolEventsFromTranscript(messages, toolEventRecords)
  state.conversation = transcript.conversation ?? null
  state.messages = attachToolEventsToMessages(messages, restoredToolEvents)
  state.toolEvents = Object.fromEntries(
    restoredToolEvents.map((event) => [event.id, toolEventFromRecord(event)]),
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
  } = {},
): Promise<{ ok: true } | { error: string }> {
  const state = ensure(id)
  state.status = 'streaming'
  state.lastError = null
  state.lastUserSendAt = Date.now()
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
  if (state)
    markAttentionResolved(
      state,
      requestId,
      decision.action === 'approve' ? 'approved' : 'rejected',
      decision.action === 'reject' ? decision.feedback : undefined,
    )
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
      if (changed) state.messages = [...state.messages]
      last.content = last.content + e.text
      const first = last.contentBlocks[0] as { type: string; text?: string }
      if (first && first.type === 'text') first.text = last.content
    })
    .with({ _tag: 'assistant.thinking' }, (e) => {
      const { message: last, changed } = ensureStreamingAssistantMessage(
        state.messages,
        e.messageId as string,
      )
      if (changed) state.messages = [...state.messages]
      last.thinking = last.thinking + e.text
    })
    .with({ _tag: 'assistant.message' }, (e) => {
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
      const finalized: SdkMessageView = {
        id: e.messageId as string,
        role: 'assistant',
        content: blocksToText(e.content),
        thinking: priorThinking,
        contentBlocks: e.content as Array<Record<string, unknown>>,
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
      const finalized: SdkMessageView = {
        id: e.messageId as string,
        role: 'assistant',
        content: blocksToText(e.content),
        thinking: priorThinking,
        contentBlocks: e.content as Array<Record<string, unknown>>,
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
    thinking: '',
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
): void {
  state.pendingAttention = state.pendingAttention.map((b) => {
    if (b.requestId !== requestId) return b
    return match(b)
      .with({ kind: 'question' }, (q) => ({ ...q, status: 'resolved' as const, answers }))
      .with({ kind: 'plan' }, (p) => ({
        ...p,
        status: (status === 'approved' ? 'approved' : 'rejected') as 'approved' | 'rejected',
        feedback,
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
