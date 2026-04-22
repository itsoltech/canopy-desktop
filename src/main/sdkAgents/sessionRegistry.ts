import { randomUUID } from 'crypto'
import type {
  AskUserQuestionAnswer,
  ConversationId,
  EffortLevel,
  PermissionMode,
  PlanDecision,
  SdkAgentEvent,
  SdkSessionId,
  ToolDecision,
} from './types'

export type SessionEventListener = (event: SdkAgentEvent) => void

export type PendingResolver =
  | { kind: 'permission'; resolve: (decision: ToolDecision) => void }
  | {
      kind: 'question'
      resolve: (answers: Record<string, AskUserQuestionAnswer>) => void
      toolUseId?: string
    }
  | { kind: 'plan'; resolve: (decision: PlanDecision) => void }

export interface ActiveSession {
  conversationId: ConversationId
  workspaceId: string
  worktreePath: string
  agentProfileId: string
  model: string
  permissionMode: PermissionMode
  /** Reasoning-effort override. `null` means "use SDK default". */
  effortLevel: EffortLevel | null
  sdkSessionId: SdkSessionId | null
  abortController: AbortController
  listeners: Set<SessionEventListener>
  /** Tools approved for the remainder of this session (Allow for session). */
  sessionAllowedTools: Set<string>
  /** Outstanding user-attention requests awaiting a renderer response. */
  pending: Map<string, PendingResolver>
  /** Track the in-flight streaming message id so deltas attach correctly. */
  currentMessageId: string | null
  /** Last user prompt — re-used by `/retry`. */
  lastUserPrompt: string | null
  createdAt: number
}

export interface ActiveSessionSeed {
  conversationId: ConversationId
  workspaceId: string
  worktreePath: string
  agentProfileId: string
  model: string
  permissionMode: PermissionMode
  effortLevel?: EffortLevel | null
}

/**
 * In-memory registry. Sessions persist across messages (same conversation =
 * same ActiveSession) but do not survive process restarts. Persistence of
 * durable state (messages, tool events) is the manager's job.
 */
export class SessionRegistry {
  private sessions = new Map<ConversationId, ActiveSession>()

  create(seed: ActiveSessionSeed): ActiveSession {
    const session: ActiveSession = {
      conversationId: seed.conversationId,
      workspaceId: seed.workspaceId,
      worktreePath: seed.worktreePath,
      agentProfileId: seed.agentProfileId,
      model: seed.model,
      permissionMode: seed.permissionMode,
      effortLevel: seed.effortLevel ?? null,
      sdkSessionId: null,
      abortController: new AbortController(),
      listeners: new Set(),
      sessionAllowedTools: new Set(),
      pending: new Map(),
      currentMessageId: null,
      lastUserPrompt: null,
      createdAt: Date.now(),
    }
    this.sessions.set(seed.conversationId, session)
    return session
  }

  get(id: ConversationId): ActiveSession | undefined {
    return this.sessions.get(id)
  }

  has(id: ConversationId): boolean {
    return this.sessions.has(id)
  }

  remove(id: ConversationId): void {
    this.sessions.delete(id)
  }

  list(): ActiveSession[] {
    return [...this.sessions.values()]
  }

  addListener(id: ConversationId, fn: SessionEventListener): () => void {
    const session = this.sessions.get(id)
    if (!session) return () => {}
    session.listeners.add(fn)
    return () => session.listeners.delete(fn)
  }

  emit(id: ConversationId, event: SdkAgentEvent): void {
    const session = this.sessions.get(id)
    if (!session) return
    for (const listener of session.listeners) listener(event)
  }

  /**
   * Resolve every pending attention request with a synthetic deny. Called on
   * session cancel so the SDK's canUseTool doesn't hang indefinitely.
   */
  rejectAllPending(id: ConversationId, reason = 'Session cancelled.'): void {
    const session = this.sessions.get(id)
    if (!session) return
    for (const [, pending] of session.pending) {
      if (pending.kind === 'permission') pending.resolve('deny')
      else if (pending.kind === 'plan') pending.resolve({ action: 'reject', feedback: reason })
      else pending.resolve({})
    }
    session.pending.clear()
  }
}

// --- waitPending helpers ---
//
// Small typed constructors for the three canUseTool flows. Each returns a
// fresh requestId (used as the wire correlation id) and a promise the caller
// awaits. The corresponding manager.respond* handler resolves the promise.

export interface PendingRegistration<T> {
  requestId: string
  promise: Promise<T>
}

export function registerPendingPermission(
  session: ActiveSession,
): PendingRegistration<ToolDecision> {
  const requestId = randomUUID()
  const promise = new Promise<ToolDecision>((resolve) => {
    session.pending.set(requestId, { kind: 'permission', resolve })
  })
  return { requestId, promise }
}

export function registerPendingQuestion(
  session: ActiveSession,
  toolUseId?: string,
): PendingRegistration<Record<string, AskUserQuestionAnswer>> {
  const requestId = randomUUID()
  const promise = new Promise<Record<string, AskUserQuestionAnswer>>((resolve) => {
    session.pending.set(requestId, { kind: 'question', resolve, toolUseId })
  })
  return { requestId, promise }
}

export function registerPendingPlan(session: ActiveSession): PendingRegistration<PlanDecision> {
  const requestId = randomUUID()
  const promise = new Promise<PlanDecision>((resolve) => {
    session.pending.set(requestId, { kind: 'plan', resolve })
  })
  return { requestId, promise }
}
