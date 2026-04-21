import { randomUUID } from 'crypto'
import { match, P } from 'ts-pattern'
import type { Database } from '../db/Database'
import type { ConversationStore } from '../db/ConversationStore'
import type { SdkMessageStore } from '../db/SdkMessageStore'
import type { SdkToolEventStore } from '../db/SdkToolEventStore'
import type { SdkAttachmentStore } from '../db/SdkAttachmentStore'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { ProfileStore } from '../profiles/ProfileStore'
import type {
  AskUserQuestionAnswer,
  Attachment,
  ConversationId,
  PermissionMode,
  PlanDecision,
  Question,
  SdkAgentEvent,
  ToolDecision,
} from './types'
import type { SdkAgentError } from './errors'
import { sdkAgentErrorMessage, toSdkAgentError } from './errors'
import { SessionRegistry, type SessionEventListener } from './sessionRegistry'
import type { CanUseToolCallback, LlmProvider } from './providers/LlmProvider'
import { AnthropicProvider } from './providers/AnthropicProvider'
import type { Conversation, SdkMessageRecord } from '../db/sdkAgentRows'

export interface SdkAgentManagerDeps {
  database: Database
  conversationStore: ConversationStore
  messageStore: SdkMessageStore
  toolEventStore: SdkToolEventStore
  attachmentStore: SdkAttachmentStore
  preferencesStore: PreferencesStore
  profileStore: ProfileStore
}

export interface CreateSessionParams {
  workspaceId: string
  worktreePath: string
  agentProfileId: string
}

export interface SendMessageParams {
  conversationId: ConversationId
  text: string
  attachments?: Attachment[]
  /** Optional per-message model override (slash command /model). */
  modelOverride?: string
  /** Optional per-message permission-mode override (slash command /mode). */
  permissionModeOverride?: PermissionMode
}

/**
 * Top-level service for the SDK-agent backend. Wires together DB stores,
 * session registry, and the active provider. No IPC registration here —
 * that lives in Phase 4's `ipcHandlers.ts`.
 */
export class SdkAgentManager {
  private readonly deps: SdkAgentManagerDeps
  private readonly registry = new SessionRegistry()
  private readonly providers = new Map<string, LlmProvider>([
    ['anthropic', new AnthropicProvider()],
  ])

  constructor(deps: SdkAgentManagerDeps) {
    this.deps = deps
  }

  // --- Registry passthrough ---

  subscribe(id: ConversationId, listener: SessionEventListener): () => void {
    return this.registry.addListener(id, listener)
  }

  listSessions(): ConversationId[] {
    return this.registry.list().map((s) => s.conversationId)
  }

  // --- Lifecycle ---

  async createSession(
    params: CreateSessionParams,
  ): Promise<{ conversationId: ConversationId } | { error: SdkAgentError }> {
    const profileResult = await this.deps.profileStore.getInternal(params.agentProfileId)
    if (profileResult.isErr()) {
      return {
        error: { _tag: 'profile_not_found', profileId: params.agentProfileId },
      }
    }
    const profile = profileResult.value
    const model = profile.prefs.model ?? 'sonnet'
    const permissionMode = (profile.prefs.permissionMode ?? 'default') as PermissionMode

    const conversation = this.deps.conversationStore.create({
      workspaceId: params.workspaceId,
      worktreePath: params.worktreePath,
      agentProfileId: params.agentProfileId,
      model,
      permissionMode,
    })

    this.registry.create({
      conversationId: conversation.id,
      workspaceId: conversation.workspaceId,
      worktreePath: conversation.worktreePath,
      agentProfileId: conversation.agentProfileId,
      model: conversation.model,
      permissionMode: conversation.permissionMode,
    })

    return { conversationId: conversation.id }
  }

  closeSession(id: ConversationId): void {
    const session = this.registry.get(id)
    if (!session) return
    session.abortController.abort()
    this.registry.rejectAllPending(id, 'Session closed.')
    this.registry.remove(id)
  }

  cancel(id: ConversationId): void {
    const session = this.registry.get(id)
    if (!session) return
    session.abortController.abort()
    this.registry.rejectAllPending(id, 'Session cancelled.')
    this.registry.emit(id, {
      _tag: 'session.end',
      sessionId: id,
      reason: 'aborted',
    })
    this.deps.conversationStore.updateStatus(id, 'cancelled')
  }

  // --- Attention responses (resolve pending promises) ---

  respondPermission(id: ConversationId, requestId: string, decision: ToolDecision): void {
    const pending = this.registry.get(id)?.pending.get(requestId)
    if (!pending || pending.kind !== 'permission') return
    pending.resolve(decision)
    this.registry.get(id)?.pending.delete(requestId)
  }

  respondQuestion(
    id: ConversationId,
    requestId: string,
    answers: Record<string, AskUserQuestionAnswer>,
  ): void {
    const pending = this.registry.get(id)?.pending.get(requestId)
    if (!pending || pending.kind !== 'question') return
    pending.resolve(answers)
    this.registry.get(id)?.pending.delete(requestId)
  }

  respondPlan(id: ConversationId, requestId: string, decision: PlanDecision): void {
    const pending = this.registry.get(id)?.pending.get(requestId)
    if (!pending || pending.kind !== 'plan') return
    pending.resolve(decision)
    this.registry.get(id)?.pending.delete(requestId)
  }

  // --- Reads ---

  getConversation(id: ConversationId): Conversation | undefined {
    return this.deps.conversationStore.get(id)
  }

  listConversations(workspaceId: string): Conversation[] {
    return this.deps.conversationStore.listByWorkspace(workspaceId)
  }

  getTranscript(id: ConversationId): {
    conversation: Conversation | undefined
    messages: SdkMessageRecord[]
  } {
    return {
      conversation: this.deps.conversationStore.get(id),
      messages: this.deps.messageStore.listByConversation(id),
    }
  }

  deleteConversation(id: ConversationId): void {
    this.closeSession(id)
    this.deps.conversationStore.hardDelete(id)
  }

  // --- The write path ---

  async sendMessage(params: SendMessageParams): Promise<void | { error: SdkAgentError }> {
    const session = this.registry.get(params.conversationId)
    if (!session) return { error: { _tag: 'profile_not_found' } }

    const profileResult = await this.deps.profileStore.getInternal(session.agentProfileId)
    if (profileResult.isErr()) {
      return { error: { _tag: 'profile_not_found', profileId: session.agentProfileId } }
    }
    const profile = profileResult.value
    if (!profile.apiKey) {
      return { error: { _tag: 'auth_missing' } }
    }

    // Persist the user message.
    this.deps.messageStore.append({
      conversationId: params.conversationId,
      role: 'user',
      content: params.text,
      contentBlocks: [{ type: 'text', text: params.text }],
    })
    session.lastUserPrompt = params.text

    const model = params.modelOverride ?? session.model
    const permissionMode = params.permissionModeOverride ?? session.permissionMode

    const provider = this.providers.get('anthropic')
    if (!provider) {
      return { error: { _tag: 'sdk_internal', message: 'no provider registered' } }
    }

    // Configure the env just like commitMessageGenerator does, scoped to this call.
    const savedEnv = applyEnv(profile.apiKey, profile.prefs)
    try {
      const startedAt = Date.now()
      const canUseTool = this.buildCanUseTool(params.conversationId)
      const iterResult = await provider.query({
        conversationId: params.conversationId,
        prompt: params.text,
        attachments: params.attachments,
        model,
        permissionMode,
        appendSystemPrompt: profile.prefs.appendSystemPrompt,
        cwd: session.worktreePath,
        apiKey: profile.apiKey,
        context: {
          canUseTool,
          signal: session.abortController.signal,
        },
        resume: session.sdkSessionId ?? undefined,
      })

      if (iterResult.isErr()) {
        this.emitAndPersistError(params.conversationId, iterResult.error)
        return { error: iterResult.error }
      }

      for await (const ev of iterResult.value) {
        this.handleEvent(params.conversationId, ev, startedAt)
      }
    } catch (e) {
      const error = toSdkAgentError(e)
      this.emitAndPersistError(params.conversationId, error)
      return { error }
    } finally {
      restoreEnv(savedEnv)
    }
    return
  }

  // --- Private helpers ---

  private handleEvent(id: ConversationId, ev: SdkAgentEvent, _startedAt: number): void {
    match(ev)
      .with({ _tag: 'session.init' }, (e) => {
        const session = this.registry.get(id)
        if (session) session.sdkSessionId = e.sdkSessionId
        this.deps.conversationStore.setSdkSessionId(id, e.sdkSessionId)
      })
      .with({ _tag: 'assistant.message' }, (e) => {
        this.deps.messageStore.append({
          id: e.messageId,
          conversationId: id,
          role: 'assistant',
          content: contentToPlain(e.content),
          contentBlocks: e.content,
          tokensIn: e.tokensIn ?? null,
          tokensOut: e.tokensOut ?? null,
        })
        this.deps.conversationStore.touch(id)
      })
      .with({ _tag: 'tool.start' }, (e) => {
        const latest = this.deps.messageStore.getLatest(id)
        if (!latest) return
        this.deps.toolEventStore.start({
          id: e.toolEventId,
          messageId: latest.id,
          conversationId: id,
          toolName: e.name,
          input: e.input,
        })
      })
      .with({ _tag: 'tool.result' }, (e) => {
        this.deps.toolEventStore.complete({
          id: e.toolEventId,
          resultText: e.result,
          isError: e.isError,
          durationMs: e.durationMs,
        })
      })
      .with({ _tag: 'session.end' }, (e) => {
        const finalStatus = e.reason === 'completed' ? 'ended' : 'error'
        this.deps.conversationStore.updateStatus(id, finalStatus)
      })
      .otherwise(() => {})

    this.registry.emit(id, ev)
  }

  private emitAndPersistError(id: ConversationId, error: SdkAgentError): void {
    this.registry.emit(id, { _tag: 'error', sessionId: id, error })
    this.deps.messageStore.append({
      conversationId: id,
      role: 'system',
      content: `Error: ${sdkAgentErrorMessage(error)}`,
      contentBlocks: [{ type: 'text', text: sdkAgentErrorMessage(error) }],
    })
    this.deps.conversationStore.updateStatus(id, 'error')
  }

  private buildCanUseTool(id: ConversationId): CanUseToolCallback {
    return async (toolName, input, ctx) => {
      if (ctx.signal.aborted) {
        return { behavior: 'deny', message: 'Aborted.' }
      }

      const session = this.registry.get(id)
      if (!session) return { behavior: 'deny', message: 'Session gone.' }

      if (toolName === 'AskUserQuestion') {
        const requestId = randomUUID()
        const questions = (input.questions as Question[]) ?? []
        const answers = await new Promise<Record<string, AskUserQuestionAnswer>>((resolve) => {
          session.pending.set(requestId, { kind: 'question', resolve })
          this.registry.emit(id, {
            _tag: 'ask_user_question',
            sessionId: id,
            requestId,
            questions,
          })
        })
        return {
          behavior: 'allow',
          updatedInput: { ...input, answers },
        }
      }

      if (toolName === 'ExitPlanMode') {
        const requestId = randomUUID()
        const plan = typeof input.plan === 'string' ? input.plan : ''
        const decision = await new Promise<PlanDecision>((resolve) => {
          session.pending.set(requestId, { kind: 'plan', resolve })
          this.registry.emit(id, {
            _tag: 'plan_mode_exit',
            sessionId: id,
            requestId,
            plan,
          })
        })
        return match(decision)
          .with({ action: 'approve' }, () => ({
            behavior: 'allow' as const,
            updatedInput: input,
          }))
          .with({ action: 'reject' }, (d) => ({
            behavior: 'deny' as const,
            message: d.feedback ?? 'Changes requested.',
          }))
          .exhaustive()
      }

      if (session.sessionAllowedTools.has(toolName)) {
        return { behavior: 'allow', updatedInput: input }
      }

      const requestId = randomUUID()
      const decision = await new Promise<ToolDecision>((resolve) => {
        session.pending.set(requestId, { kind: 'permission', resolve })
        this.registry.emit(id, {
          _tag: 'tool.permission_request',
          sessionId: id,
          requestId,
          toolName,
          input,
        })
      })
      return match(decision)
        .with('allow-once', () => ({ behavior: 'allow' as const, updatedInput: input }))
        .with('allow-session', () => {
          session.sessionAllowedTools.add(toolName)
          return { behavior: 'allow' as const, updatedInput: input }
        })
        .with('deny', () => ({ behavior: 'deny' as const, message: 'Denied by user.' }))
        .exhaustive()
    }
  }
}

// --- Plain-text rendering for FTS5 indexing ---

function contentToPlain(blocks: readonly { type: string; [k: string]: unknown }[]): string {
  const parts: string[] = []
  for (const b of blocks) {
    match(b)
      .with({ type: 'text', text: P.string }, (t) => parts.push(t.text))
      .with({ type: 'tool_use', name: P.string }, (t) => parts.push(`[tool:${t.name}]`))
      .with({ type: 'tool_result' }, () => {})
      .otherwise(() => {})
  }
  return parts.join('\n\n')
}

// --- Env scoping (mirrors commitMessageGenerator) ---

interface SavedEnv {
  before: Record<string, string | undefined>
}

function applyEnv(apiKey: string, prefs: { baseUrl?: string; provider?: string }): SavedEnv {
  const overrides: Record<string, string> = { ANTHROPIC_API_KEY: apiKey }
  if (prefs.baseUrl) overrides.ANTHROPIC_BASE_URL = prefs.baseUrl
  if (prefs.provider === 'bedrock') overrides.CLAUDE_CODE_USE_BEDROCK = '1'
  if (prefs.provider === 'vertex') overrides.CLAUDE_CODE_USE_VERTEX = '1'
  if (prefs.provider === 'foundry') overrides.CLAUDE_CODE_USE_FOUNDRY = '1'

  const before: Record<string, string | undefined> = {}
  for (const [k, v] of Object.entries(overrides)) {
    before[k] = process.env[k]
    process.env[k] = v
  }
  return { before }
}

function restoreEnv(saved: SavedEnv): void {
  for (const [k, v] of Object.entries(saved.before)) {
    if (v !== undefined) process.env[k] = v
    else delete process.env[k]
  }
}
