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
    }
  | {
      kind: 'plan'
      requestId: string
      messageId: string | null
      plan: string
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

export interface SdkMessageView {
  id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  contentBlocks: Array<Record<string, unknown>>
  tokensIn: number | null
  tokensOut: number | null
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

export type SdkSessionStatus = 'idle' | 'streaming' | 'ended' | 'error' | 'cancelled' | 'starting'

export interface SdkSessionState {
  conversationId: string
  conversation: SdkConversation | null
  messages: SdkMessageView[]
  toolEvents: Record<string, SdkToolEventView>
  pendingAttention: AttentionBlock[]
  sdkSessionId: string | null
  status: SdkSessionStatus
  tokensIn: number
  tokensOut: number
  costUsd: number
  lastError: string | null
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
    pendingAttention: [],
    sdkSessionId: null,
    status: 'idle',
    ...EMPTY_USAGE,
    lastError: null,
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
  state.conversation = transcript.conversation ?? null
  state.messages = transcript.messages.map(messageFromRecord)
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
  // Optimistic: push the user message; the backend event will not emit a user
  // echo, so this is authoritative.
  state.messages = [
    ...state.messages,
    {
      id: `local-${Date.now()}`,
      role: 'user',
      content: text,
      contentBlocks: [{ type: 'text', text }],
      tokensIn: null,
      tokensOut: null,
      createdAt: nowIso(),
      toolEventIds: [],
    },
  ]
  const result = await window.api.sdkAgent.send({
    conversationId: id,
    text,
    attachments: options.attachments,
    modelOverride: options.modelOverride,
    permissionModeOverride: options.permissionModeOverride,
  })
  if ('error' in result) {
    state.status = 'error'
    state.lastError = result.error
  }
  return result
}

export async function cancel(id: string): Promise<void> {
  await window.api.sdkAgent.cancel(id)
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
    if (state) markAttentionResolved(state, requestId, 'resolved')
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
      let last = state.messages[state.messages.length - 1]
      if (!last || last.role !== 'assistant' || last.id !== e.messageId) {
        last = {
          id: e.messageId as string,
          role: 'assistant',
          content: '',
          contentBlocks: [{ type: 'text', text: '' }],
          tokensIn: null,
          tokensOut: null,
          createdAt: nowIso(),
          toolEventIds: [],
        }
        state.messages = [...state.messages, last]
      }
      last.content = last.content + e.text
      const first = last.contentBlocks[0] as { type: string; text?: string }
      if (first && first.type === 'text') first.text = last.content
    })
    .with({ _tag: 'assistant.message' }, (e) => {
      const idx = state.messages.findIndex(
        (m) => m.role === 'assistant' && m.id === (e.messageId as string),
      )
      const finalized: SdkMessageView = {
        id: e.messageId as string,
        role: 'assistant',
        content: blocksToText(e.content),
        contentBlocks: e.content as Array<Record<string, unknown>>,
        tokensIn: e.tokensIn ?? null,
        tokensOut: e.tokensOut ?? null,
        createdAt: nowIso(),
        toolEventIds: idx >= 0 ? state.messages[idx].toolEventIds : [],
      }
      if (idx >= 0) state.messages[idx] = finalized
      else state.messages = [...state.messages, finalized]
    })
    .with({ _tag: 'tool.start' }, (e) => {
      state.toolEvents[e.toolEventId] = {
        id: e.toolEventId,
        messageId: lastAssistantId(state),
        toolName: e.name,
        input: e.input,
        result: null,
        isError: false,
        durationMs: null,
        status: 'running',
      }
      const last = state.messages[state.messages.length - 1]
      if (last && last.role === 'assistant' && !last.toolEventIds.includes(e.toolEventId)) {
        last.toolEventIds = [...last.toolEventIds, e.toolEventId]
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
          plan: e.plan,
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
    .with({ _tag: 'subagent.start' }, () => {})
    .with({ _tag: 'subagent.event' }, () => {})
    .with({ _tag: 'subagent.end' }, () => {})
    .exhaustive()
}

function messageFromRecord(record: SdkMessageRecord): SdkMessageView {
  return {
    id: record.id as string,
    role: record.role,
    content: record.content,
    contentBlocks: record.contentBlocks as Array<Record<string, unknown>>,
    tokensIn: record.tokensIn,
    tokensOut: record.tokensOut,
    createdAt: record.createdAt,
    toolEventIds: [],
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
): void {
  state.pendingAttention = state.pendingAttention.map((b) => {
    if (b.requestId !== requestId) return b
    return match(b)
      .with({ kind: 'question' }, (q) => ({ ...q, status: 'resolved' as const }))
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
