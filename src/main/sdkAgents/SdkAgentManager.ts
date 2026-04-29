import fs from 'fs'
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
  ContentBlock,
  ConversationId,
  EffortLevel,
  PermissionMode,
  PlanAllowedPrompt,
  PlanDecision,
  Question,
  SdkAgentEvent,
  ToolDecision,
} from './types'
import { normalizeEffortLevel, normalizePermissionMode } from './types'
import type { SdkAgentError } from './errors'
import { sdkAgentErrorMessage, toSdkAgentError } from './errors'
import {
  SessionRegistry,
  registerPendingPermission,
  registerPendingPlan,
  registerPendingQuestion,
  type ActiveSession,
  type SessionEventListener,
} from './sessionRegistry'
import type { CanUseToolCallback, LlmProvider } from './providers/LlmProvider'
import { AnthropicProvider, shapeQuestionAllowPayload } from './providers/AnthropicProvider'
import { OpenAiCodexProvider } from './providers/OpenAiCodexProvider'
import type { Conversation, SdkMessageRecord, SdkToolEventRecord } from '../db/sdkAgentRows'
import type { ConversationSearchHit } from '../db/ConversationStore'
import { buildClaudeProviderEnv } from '../agents/claudeProviderEnv'

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
  /**
   * Optional per-message reasoning-effort override. Ignored for models that
   * do not advertise `reasoning: true` in the catalog.
   */
  effortLevelOverride?: EffortLevel | null
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
    ['openai', new OpenAiCodexProvider()],
  ])

  constructor(deps: SdkAgentManagerDeps) {
    this.deps = deps
  }

  // --- Registry passthrough ---

  subscribe(id: ConversationId, listener: SessionEventListener): () => void {
    this.ensureActiveSession(id)
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
    const model = profile.prefs.model ?? defaultModelForAgentType(profile.agentType)
    const permissionMode = normalizePermissionMode(profile.prefs.permissionMode)
    const effortLevel = normalizeEffortLevel(profile.prefs.effortLevel)

    const conversation = this.deps.conversationStore.create({
      workspaceId: params.workspaceId,
      worktreePath: params.worktreePath,
      agentProfileId: params.agentProfileId,
      model,
      permissionMode,
    })
    if (effortLevel) {
      this.deps.conversationStore.setEffortLevel(conversation.id, effortLevel)
    }

    this.registry.create({
      conversationId: conversation.id,
      workspaceId: conversation.workspaceId,
      worktreePath: conversation.worktreePath,
      agentProfileId: conversation.agentProfileId,
      model: conversation.model,
      permissionMode: conversation.permissionMode,
      effortLevel,
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

  updateConversation(
    id: ConversationId,
    patch: {
      model?: string
      permissionMode?: PermissionMode
      effortLevel?: EffortLevel | null
    },
  ): void {
    const session = this.registry.get(id)
    if (patch.model !== undefined) {
      this.deps.conversationStore.setModel(id, patch.model)
      if (session) session.model = patch.model
    }
    if (patch.permissionMode !== undefined) {
      const permissionMode = normalizePermissionMode(patch.permissionMode)
      this.deps.conversationStore.setPermissionMode(id, permissionMode)
      if (session) session.permissionMode = permissionMode
    }
    if (patch.effortLevel !== undefined) {
      const effortLevel =
        patch.effortLevel === null ? null : normalizeEffortLevel(patch.effortLevel)
      this.deps.conversationStore.setEffortLevel(id, effortLevel)
      if (session) session.effortLevel = effortLevel
    }
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
    const session = this.registry.get(id)
    const pending = session?.pending.get(requestId)
    if (!session) {
      console.warn('[sdk-agent][question] response dropped: session missing', { id, requestId })
      return
    }
    if (!pending) {
      console.warn('[sdk-agent][question] response dropped: pending request missing', {
        id,
        requestId,
        pendingRequestIds: [...session.pending.keys()],
      })
      return
    }
    if (pending.kind !== 'question') {
      console.warn('[sdk-agent][question] response dropped: pending kind mismatch', {
        id,
        requestId,
        pendingKind: pending.kind,
      })
      return
    }
    console.info('[sdk-agent][question] response received', {
      id,
      requestId,
      answerKeys: Object.keys(answers),
    })
    if (pending.toolUseId) {
      this.deps.toolEventStore.saveQuestionAnswers(pending.toolUseId, answers)
    }
    pending.resolve(answers)
    session.pending.delete(requestId)
    console.info('[sdk-agent][question] response resolved', {
      id,
      requestId,
      pendingCount: session.pending.size,
    })
  }

  respondPlan(id: ConversationId, requestId: string, decision: PlanDecision): void {
    const session = this.registry.get(id)
    const pending = session?.pending.get(requestId)
    if (!pending || pending.kind !== 'plan') return
    if (decision.action === 'approve') {
      const permissionMode = normalizePermissionMode(decision.permissionMode)
      this.deps.conversationStore.setPermissionMode(id, permissionMode)
      if (session) session.permissionMode = permissionMode
    }
    pending.resolve(decision)
    session?.pending.delete(requestId)
  }

  // --- Reads ---

  getConversation(id: ConversationId): Conversation | undefined {
    return this.deps.conversationStore.get(id)
  }

  listConversations(workspaceId: string): Conversation[] {
    return this.deps.conversationStore.listByWorkspace(workspaceId)
  }

  listConversationsByWorktree(workspaceId: string, worktreePath: string): Conversation[] {
    return this.deps.conversationStore.listByWorktree(workspaceId, worktreePath)
  }

  /** Close active sessions + hard-delete every conversation for a worktree. */
  deleteConversationsByWorktree(workspaceId: string, worktreePath: string): number {
    const ids = this.deps.conversationStore.listIdsByWorktree(workspaceId, worktreePath)
    for (const id of ids) this.deleteConversation(id)
    return ids.length
  }

  /**
   * Cascade-delete every conversation rooted at this worktree path, across all
   * workspaces. Used on `git worktree remove` where the caller does not carry
   * a workspaceId.
   */
  deleteConversationsByWorktreePath(worktreePath: string): number {
    const ids = this.deps.conversationStore.listIdsByWorktreePath(worktreePath)
    for (const id of ids) this.deleteConversation(id)
    return ids.length
  }

  getTranscript(id: ConversationId): {
    conversation: Conversation | undefined
    messages: SdkMessageRecord[]
    toolEvents: SdkToolEventRecord[]
  } {
    return {
      conversation: this.deps.conversationStore.get(id),
      messages: this.deps.messageStore.listByConversation(id),
      toolEvents: this.deps.toolEventStore.listByConversation(id),
    }
  }

  deleteConversation(id: ConversationId): void {
    this.closeSession(id)
    this.deps.conversationStore.hardDelete(id)
  }

  searchConversations(workspaceId: string, query: string, limit = 50): ConversationSearchHit[] {
    return this.deps.conversationStore.search(workspaceId, query, limit)
  }

  // --- The write path ---

  async sendMessage(params: SendMessageParams): Promise<void | { error: SdkAgentError }> {
    const session = this.ensureActiveSession(params.conversationId)
    if (!session) return { error: { _tag: 'profile_not_found' } }
    if (session.abortController.signal.aborted) session.abortController = new AbortController()

    const profileResult = await this.deps.profileStore.getInternal(session.agentProfileId)
    if (profileResult.isErr()) {
      return { error: { _tag: 'profile_not_found', profileId: session.agentProfileId } }
    }
    const profile = profileResult.value
    // apiKey is optional: when omitted, the SDK delegates auth to the bundled
    // Claude CLI binary (pathToClaudeCodeExecutable), which uses its own
    // stored session. Only set ANTHROPIC_API_KEY when the profile explicitly
    // provides one to override that fallback.

    let userContent: ContentBlock[]
    try {
      userContent = buildUserContent(params.text, params.attachments)
    } catch (e) {
      const error = toSdkAgentError(e)
      this.emitAndPersistError(params.conversationId, error)
      return { error }
    }
    const userMessage = this.deps.messageStore.append({
      conversationId: params.conversationId,
      role: 'user',
      content: contentToPlain(userContent),
      contentBlocks: userContent,
    })
    for (const attachment of params.attachments ?? []) {
      this.deps.attachmentStore.insert({
        id: attachment.id,
        messageId: userMessage.id,
        conversationId: params.conversationId,
        kind: attachment.kind,
        filename: attachment.filename,
        path: attachment.path,
        mimeType: attachment.mimeType,
        sizeBytes: attachment.sizeBytes,
      })
    }
    session.lastUserPrompt = params.text
    this.deps.conversationStore.updateStatus(params.conversationId, 'active')

    const model = params.modelOverride ?? session.model
    const permissionMode = params.permissionModeOverride ?? session.permissionMode
    const effortLevel =
      params.effortLevelOverride !== undefined ? params.effortLevelOverride : session.effortLevel

    const provider = this.providers.get(providerIdForAgentType(profile.agentType))
    if (!provider) {
      return { error: { _tag: 'sdk_internal', message: 'no provider registered' } }
    }

    // Configure the env just like commitMessageGenerator does, scoped to this
    // call. apiKey may be null; applyEnv handles that by only setting base URL
    // and provider env vars.
    const savedEnv =
      provider.providerId === 'anthropic' ? applyEnv(profile.apiKey, profile.prefs) : null
    try {
      const canUseTool = this.buildCanUseTool(params.conversationId)
      const iterResult = await provider.query({
        conversationId: params.conversationId,
        prompt: userContent,
        attachments: params.attachments,
        model,
        permissionMode,
        effort: effortLevel,
        appendSystemPrompt: profile.prefs.appendSystemPrompt,
        mcpServers: parseMcpServers(profile.prefs.mcpServers),
        cwd: session.worktreePath,
        apiKey: profile.apiKey ?? undefined,
        baseUrl: profile.prefs.baseUrl,
        customEnv: profile.prefs.customEnv,
        settingsJson: profile.prefs.settingsJson,
        approvalMode: profile.prefs.approvalMode,
        sandboxMode: profile.prefs.sandbox,
        context: {
          canUseTool,
          signal: session.abortController.signal,
        },
        resume: session.sdkSessionId ?? undefined,
      })

      if (iterResult.isErr()) {
        if (iterResult.error._tag === 'aborted') {
          this.emitAndPersistCancellation(params.conversationId)
          return
        }
        this.emitAndPersistError(params.conversationId, iterResult.error)
        return { error: iterResult.error }
      }

      for await (const ev of iterResult.value) {
        this.handleEvent(params.conversationId, ev)
      }
    } catch (e) {
      const error = toSdkAgentError(e)
      if (error._tag === 'aborted') {
        this.emitAndPersistCancellation(params.conversationId)
        return
      }
      this.emitAndPersistError(params.conversationId, error)
      return { error }
    } finally {
      if (savedEnv) restoreEnv(savedEnv)
    }
    return
  }

  // --- Private helpers ---

  private ensureActiveSession(id: ConversationId): ActiveSession | undefined {
    const existing = this.registry.get(id)
    if (existing) return existing

    const conversation = this.deps.conversationStore.get(id)
    if (!conversation) return undefined

    const session = this.registry.create({
      conversationId: conversation.id,
      workspaceId: conversation.workspaceId,
      worktreePath: conversation.worktreePath,
      agentProfileId: conversation.agentProfileId,
      model: conversation.model,
      permissionMode: conversation.permissionMode,
      effortLevel: conversation.effortLevel,
    })
    session.sdkSessionId = conversation.sdkSessionId
    return session
  }

  private handleEvent(id: ConversationId, ev: SdkAgentEvent): void {
    match(ev)
      .with({ _tag: 'session.init' }, (e) => {
        const session = this.registry.get(id)
        if (session) session.sdkSessionId = e.sdkSessionId
        this.deps.conversationStore.setSdkSessionId(id, e.sdkSessionId)
      })
      .with({ _tag: 'assistant.message' }, (e) => this.persistEvent(id, e))
      .with({ _tag: 'assistant.thinking' }, (e) => this.persistEvent(id, e))
      .with({ _tag: 'tool.start' }, (e) => this.persistEvent(id, e))
      .with({ _tag: 'tool.result' }, (e) => this.persistEvent(id, e))
      .with({ _tag: 'subagent.event' }, (e) => this.persistEvent(id, e.event, e.subagentId))
      .with({ _tag: 'session.end' }, (e) => {
        const finalStatus =
          e.reason === 'completed' ? 'ended' : e.reason === 'aborted' ? 'cancelled' : 'error'
        this.deps.conversationStore.updateStatus(id, finalStatus)
      })
      .otherwise(() => {})

    this.registry.emit(id, ev)
  }

  private persistEvent(
    conversationId: ConversationId,
    ev: SdkAgentEvent,
    parentSubagentId: string | null = null,
  ): void {
    match(ev)
      .with({ _tag: 'assistant.message' }, (e) => {
        const existing = this.deps.messageStore.getById(e.messageId)
        const mergedContent = mergeAssistantContentBlocks(
          existing?.contentBlocks as ContentBlock[] | undefined,
          e.content,
        )
        this.deps.messageStore.append({
          id: e.messageId,
          conversationId,
          parentSubagentId,
          role: 'assistant',
          content: contentToPlain(mergedContent),
          contentBlocks: mergedContent,
          thinking: existing?.thinking ?? null,
          tokensIn: e.tokensIn ?? null,
          tokensOut: e.tokensOut ?? null,
          model: e.model ?? null,
        })
        this.deps.conversationStore.touch(conversationId)
      })
      .with({ _tag: 'assistant.thinking' }, (e) => {
        const existing = this.deps.messageStore.getById(e.messageId)
        if (existing) {
          this.deps.messageStore.appendThinking(e.messageId, e.text)
        } else {
          this.deps.messageStore.append({
            id: e.messageId,
            conversationId,
            parentSubagentId,
            role: 'assistant',
            content: '',
            contentBlocks: [{ type: 'text', text: '' }],
            thinking: e.text,
          })
        }
        this.deps.conversationStore.touch(conversationId)
      })
      .with({ _tag: 'tool.start' }, (e) => {
        this.deps.toolEventStore.start({
          id: e.toolEventId,
          messageId: e.messageId,
          conversationId,
          parentSubagentId,
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
      .otherwise(() => {})
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

  private emitAndPersistCancellation(id: ConversationId): void {
    this.registry.emit(id, { _tag: 'session.end', sessionId: id, reason: 'aborted' })
    this.deps.messageStore.append({
      conversationId: id,
      role: 'system',
      content: 'Request interrupted by user.',
      contentBlocks: [{ type: 'text', text: 'Request interrupted by user.' }],
    })
    this.deps.conversationStore.updateStatus(id, 'cancelled')
  }

  private buildCanUseTool(id: ConversationId): CanUseToolCallback {
    return async (toolName, input, ctx) => {
      if (ctx.signal.aborted) {
        return { behavior: 'deny', message: 'Aborted.' }
      }

      const session = this.registry.get(id)
      if (!session) return { behavior: 'deny', message: 'Session gone.' }

      if (toolName === 'AskUserQuestion') {
        const { requestId, promise } = registerPendingQuestion(session, ctx.toolUseId)
        const questions = (input.questions as Question[]) ?? []
        console.info('[sdk-agent][question] request waiting', {
          id,
          requestId,
          questionCount: questions.length,
          questions: questions.map((q) => q.question),
        })
        this.registry.emit(id, {
          _tag: 'ask_user_question',
          sessionId: id,
          requestId,
          questions,
        })
        const answers = await promise
        const result = shapeQuestionAllowPayload(input, answers)
        if (ctx.toolUseId) this.deps.toolEventStore.updateInput(ctx.toolUseId, result.updatedInput)
        console.info('[sdk-agent][question] request continuing', {
          id,
          requestId,
          answerKeys: Object.keys(answers),
          formattedAnswers: result.updatedInput.answers,
        })
        return result
      }

      if (toolName === 'ExitPlanMode') {
        const { requestId, promise } = registerPendingPlan(session)
        const plan = typeof input.plan === 'string' ? input.plan : ''
        const allowedPrompts = extractPlanAllowedPrompts(input)
        this.registry.emit(id, {
          _tag: 'plan_mode_exit',
          sessionId: id,
          requestId,
          toolEventId: ctx.toolUseId,
          plan,
          allowedPrompts,
        })
        const decision = await promise
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

      const { requestId, promise } = registerPendingPermission(session)
      this.registry.emit(id, {
        _tag: 'tool.permission_request',
        sessionId: id,
        requestId,
        toolName,
        input,
      })
      const decision = await promise
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

function extractPlanAllowedPrompts(input: Record<string, unknown>): PlanAllowedPrompt[] {
  const raw = input.allowedPrompts ?? input.allowed_prompts
  if (!Array.isArray(raw)) return []

  return raw.flatMap((item) => {
    if (!item || typeof item !== 'object') return []
    const record = item as Record<string, unknown>
    if (typeof record.tool !== 'string' || typeof record.prompt !== 'string') return []
    return [{ tool: record.tool, prompt: record.prompt }]
  })
}

// --- Plain-text rendering for FTS5 indexing ---

function contentToPlain(blocks: readonly { type: string; [k: string]: unknown }[]): string {
  const parts: string[] = []
  for (const b of blocks) {
    match(b)
      .with({ type: 'text', text: P.string }, (t) => parts.push(t.text))
      .otherwise(() => {})
  }
  return parts.join('\n\n')
}

function mergeAssistantContentBlocks(
  previous: readonly ContentBlock[] | undefined,
  next: readonly ContentBlock[],
): ContentBlock[] {
  const nextHasText = next.some((block) => block.type === 'text' && block.text.length > 0)
  if (nextHasText || !previous?.length) return [...next]

  const previousTextBlocks = previous.filter(
    (block): block is Extract<ContentBlock, { type: 'text' }> =>
      block.type === 'text' && block.text.length > 0,
  )
  if (previousTextBlocks.length === 0) return [...next]

  return [...previousTextBlocks, ...next]
}

function buildUserContent(text: string, attachments: Attachment[] | undefined): ContentBlock[] {
  const blocks: ContentBlock[] = []
  if (text.length > 0) blocks.push({ type: 'text', text })
  for (const attachment of attachments ?? []) {
    blocks.push(attachmentToContentBlock(attachment))
  }
  return blocks.length > 0 ? blocks : [{ type: 'text', text: '' }]
}

function attachmentToContentBlock(attachment: Attachment): ContentBlock {
  if (attachment.kind === 'image') {
    return {
      type: 'image',
      source: {
        type: 'base64',
        media_type: normalizeImageMime(attachment.mimeType),
        data: fs.readFileSync(attachment.path).toString('base64'),
      },
      filename: attachment.filename,
    }
  }

  return {
    type: 'text',
    text: `<attachment filename="${attachment.filename}">\n${fs.readFileSync(
      attachment.path,
      'utf8',
    )}\n</attachment>`,
  }
}

function normalizeImageMime(mimeType: string): string {
  if (mimeType === 'image/jpg') return 'image/jpeg'
  if (
    mimeType === 'image/png' ||
    mimeType === 'image/jpeg' ||
    mimeType === 'image/gif' ||
    mimeType === 'image/webp'
  ) {
    return mimeType
  }
  return 'image/png'
}

// --- Env scoping (mirrors commitMessageGenerator) ---

interface SavedEnv {
  before: Record<string, string | undefined>
}

function applyEnv(
  apiKey: string | null,
  prefs: {
    model?: string
    baseUrl?: string
    provider?: string
    claudeProviderPreset?: string
    providerModel?: string
    providerOpusModel?: string
    providerSonnetModel?: string
    providerHaikuModel?: string
    customEnv?: string
  },
): SavedEnv {
  const overrides = buildClaudeProviderEnv(apiKey, prefs)
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

/**
 * Parse the raw `mcpServers` JSON from a profile. Silent on invalid input —
 * the Phase 8 preferences UI owns validation at write-time, and malformed
 * persisted data should not prevent a session from starting.
 */
function parseMcpServers(raw: string | undefined): Record<string, unknown> | undefined {
  if (!raw) return undefined
  try {
    const parsed = JSON.parse(raw) as unknown
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>
    }
    return undefined
  } catch {
    return undefined
  }
}

function defaultModelForAgentType(agentType: string): string {
  return agentType === 'codex-sdk' ? 'codex' : 'sonnet'
}

function providerIdForAgentType(agentType: string): 'anthropic' | 'openai' {
  return agentType === 'codex-sdk' ? 'openai' : 'anthropic'
}
