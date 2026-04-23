# SPEC: SDK-Based Agent Services (Anthropic Claude Agent SDK)

## Requirements

Wire up in-process Claude Agent SDK (`@anthropic-ai/claude-agent-sdk` v0.2.100, already installed and used today only by `src/main/ai/commitMessageGenerator.ts`) into Canopy as a full interactive-agent backend that drives the chat UI molecules shipped in the previous pass (`src/renderer/src/components/chat/`). This is a new service **sibling** to the existing PTY-spawned CLI agent system in `src/main/agents/` — not a replacement.

### Functional scope

- **Session scope:** one SDK session per chat pane (same lifecycle as `PtyManager` sessions today).
- **Persistence:** full-fidelity transcripts persisted to SQLite — new `conversations`, `messages`, `tool_events` tables with FTS5 full-text search over `messages.content`.
- **Auth & model:** reuse `src/main/profiles/` (ProfileStore). Anthropic API key encrypted via Electron `safeStorage` (same as PTY `claude` adapter). Model + `permissionMode` + `appendSystemPrompt` + `mcpServers` come from the selected profile's `ProfilePrefs`. User can override the model per message via `ModelPickerInline`.
- **Permission mode default:** read from `ProfilePrefs.permissionMode` (same field the PTY `claude.ts` adapter reads via `prefs.get('claude.permissionMode')`).
- **Tools:** core Read / Edit / Write / Glob / Grep / Bash / WebSearch / WebFetch, plus the SDK built-ins `AskUserQuestion` + `ExitPlanMode`. All enabled by default; permission mode gates execution.
- **CWD:** each session is spawned with cwd = the pane's worktree root (the same path the PTY agent would launch under).
- **Streaming:** token-level streaming into the UI. Assistant messages render deltas as they arrive.
- **Concurrency:** one SDK session per pane, no hard cap — memory and API quota are the user's constraint.
- **Cancellation:** Stop button + ⌘. shortcut trigger `AbortController.abort()`. The partial assistant message stays visible with a "stopped" marker.
- **AskUserQuestion:** rendered **inline** in the message stream using the `QuestionnaireBlock` molecule. After answer, the block becomes read-only.
- **ExitPlanMode:** rendered inline using `PlanApprovalBlock`. Approve → call `query.setPermissionMode('acceptEdits')` and continue. Reject with optional feedback → push a new user message that contains the feedback.
- **Tool permission prompts:** `Allow once` / `Allow for session` / `Deny` palette rendered inline via `ToolPermissionBlock`. Session-scoped decisions live in an in-memory `Map<toolName, 'session-allow'>`. SDK-suggested `PermissionUpdate` hints are ignored in MVP.
- **Destructive detection:** trust the SDK's `permissionMode` + our allowlist. No extra Canopy-side classifier; no reuse of `security-hook.py` here.
- **Input:** multi-line textarea with ⌘↵ submit (`ChatInput` molecule), text file attachments inlined as fenced code blocks in user messages, image attachments (base64-encoded at send-time; files copied to `~/.config/Canopy/attachments/<session>/<uuid>.<ext>` on attach), and slash-command quick actions (`/new`, `/clear`, `/model`, `/mode`, `/retry`).
- **New pane UX:** empty message stream + focused input. No starter templates in MVP.
- **Conversation title:** auto-generated after first assistant message by a short Claude Haiku call (reusing `commitMessageGenerator.ts`'s approach).
- **History:** soft-deleted with undo — correction: **hard delete with confirmation** (per answers). FTS5-backed search in a sidebar (basic UI).
- **Error surfacing:** inline `AssistantErrorBlock` molecule with `_tag`-discriminated errors. 429 → auto-retry with exponential backoff (1s / 4s / 16s; respect `retry-after` header).
- **Rate/cost:** per-message `TokenCount` in the message meta row + session total in the pane footer with an estimated USD based on model pricing.
- **Logs:** Canopy's existing main-process logger + per-session log file at `~/.config/Canopy/logs/sdk/<canopy-session-id>.log` with 7-day rotation.

### Abstraction posture

Although MVP ships with Anthropic only, introduce a provider-agnostic `LlmProvider` interface from day one. `AnthropicProvider` is the first concrete implementation. Future providers (OpenAI, Gemini) plug in without changing renderer or IPC surfaces.

### Architectural constraints

- `query()` runs in the **main process**. Renderer never imports the SDK.
- Events cross IPC as a typed `_tag`-discriminated union (`ts-pattern`-matched renderer-side).
- Conversations are keyed by a Canopy-generated UUIDv4 (`conversations.id`). SDK's `session_id` is captured from the first `system.init` event and stored in a separate nullable column (`sdk_session_id`).
- Sub-agents (Claude's `Task` tool) render as nested `SubAgentRun` blocks in the parent transcript.
- MCP servers are configurable per-profile (new `mcpServers` JSON field in `ProfilePrefs`). Passed to SDK's `mcpServers` option verbatim.
- Session resume: reopening a prior conversation shows the transcript read-only; sending a new message calls `query({ resume: sdk_session_id, ... })`.
- Pane close with pending permission / question → `ConfirmDialog` asks to abort or keep. Aborting marks the last open block as `cancelled`.

### Out of scope for MVP

- Automated tests (unit/integration/e2e).
- Dogfood feature-flag gating: ship behind `prefs.experimental.sdkAgents` (default off) and flip once the user has dogfooded it.
- Multi-provider implementations beyond `AnthropicProvider`.
- Deprecation of the existing PTY `claude` adapter.
- Complex permission-rule persistence across sessions (`suggestions` accepted as rules → deferred).

---

## Project Conventions

Key patterns to follow, derived from `CLAUDE.md` and the codebase:

- **Error handling:** `neverthrow` `Result<T, E>` / `ResultAsync<T, E>` in all business logic. Typed error unions with `_tag` discriminants in per-domain `errors.ts` files. Wrap external async via `fromExternalCall()` from `src/main/errors.ts`. `try/catch` only at process boundaries (PTY cleanup, HTTP parsing, untrusted `JSON.parse`, `contextBridge` init, renderer event handlers).
- **Pattern matching:** `ts-pattern` (`match` / `with` / `P.union` / `P.when`) over `switch` for discriminated unions. `.exhaustive()` when all cases are handled.
- **Svelte 5 runes:** `$props`, `$state`, `$derived`, `$derived.by`, `$effect`, `Snippet` for children. No `createEventDispatcher`. Use `SvelteSet` from `svelte/reactivity` for reactive sets.
- **IPC:** bare `ipcMain.handle(channel, handler)` + `ipcRenderer.invoke(channel, payload)`. Channels namespaced by colon (e.g., `sdkAgent:create`). Types defined in `src/preload/index.ts` + `src/preload/index.d.ts`. Push events via `ipcSend()` helper.
- **Persistence:** raw SQL via `better-sqlite3` prepared statements. Migrations are an append-only array in `src/main/db/Database.ts` constructor. Each table has a dedicated `*Store` wrapper class in `src/main/db/` or a domain subfolder.
- **Credentials:** Electron `safeStorage.encryptString()` → base64 → SQLite. OS keychain via `KeychainTokenStore` for cross-app secrets (not used by profile API keys — those go in the `agent_profiles` table encrypted).
- **Managers:** singletons instantiated once at `app.whenReady()` in `src/main/index.ts`. Passed as dependencies to `registerIpcHandlers()` in `src/main/ipc/handlers.ts`.
- **Agent events:** existing normalized event shape in `src/main/agents/types.ts` (`NormalizedHookEvent`). SDK events will NOT reuse this shape — see Phase 1 — because the SDK's event taxonomy is richer than the PTY hook-event set. But the renderer stores (`agentState.svelte.ts`) will be extended to display both.
- **Renderer stores:** `.svelte.ts` files under `src/renderer/src/lib/stores/`. `$state`-backed state exposed as object/functions; no classes required.
- **Code style:** single quotes, no semicolons, 100-char print width, 2-space indent, LF endings. `import type` for type-only imports (`verbatimModuleSyntax: true`).
- **Commits:** conventional (`feat:`, `fix:`, `chore:`, `refactor:`, etc.).
- **SPEC.md:** per project CLAUDE.md, never commit. This file is scratch.

### Representative file anchors

- IPC hub: `src/main/ipc/handlers.ts` (~97 K, all channels registered here).
- Preload bridge: `src/preload/index.ts` + `src/preload/index.d.ts`.
- Existing minimal SDK usage: `src/main/ai/commitMessageGenerator.ts` (Haiku + JSON schema).
- PTY Claude adapter (template for permissionMode read): `src/main/agents/adapters/claude.ts:174`.
- Profile data model: `src/main/profiles/types.ts`.
- Database + migrations: `src/main/db/Database.ts`.
- Agent state store (to extend for SDK sessions): `src/renderer/src/lib/agents/agentState.svelte.ts`.
- Chat molecules already built: `src/renderer/src/components/chat/molecules/`.
- Inspector: `src/renderer/src/components/agents/AgentInspector.svelte`.

---

## Complexity Assessment

- **Scope:** very large — net-new service, new IPC namespace, new DB tables + FTS5, new pane kind, rich UI wiring for three distinct attention flows, provider-agnostic abstraction for later multi-provider work.
- **Areas affected:**
  - `src/main/sdkAgents/` (new)
  - `src/main/db/` (new stores + migrations)
  - `src/main/ipc/handlers.ts` (new channel registrations)
  - `src/main/profiles/` (extend `ProfilePrefs` with `mcpServers`)
  - `src/preload/index.ts` + `index.d.ts` (new API surface)
  - `src/renderer/src/lib/stores/sdkAgentSessions.svelte.ts` (new)
  - `src/renderer/src/lib/agents/agentState.svelte.ts` (extend to include SDK sessions in the inspector)
  - `src/renderer/src/components/chat/organisms/` (new `SdkChatPane.svelte` + `MessageStream.svelte`)
  - `src/renderer/src/components/chat/molecules/` (new `AssistantErrorBlock.svelte`; tweak existing molecules only if needed)
  - `src/renderer/src/lib/stores/tabs.svelte.ts` (register new pane kind)
  - `src/renderer/src/components/agents/AgentInspector.svelte` (extend to render SDK sessions)
  - `src/renderer-shared/styles/tokens.css` (no changes expected)
  - New attachment storage directory under Electron `userData`

---

## Implementation Plan

### Phase 1 — Foundational types + feature flag

**Goal:** freeze the public type contracts the rest of the system references. No runtime behavior.

**Locations:**

- `src/main/sdkAgents/types.ts` (new)
- `src/main/sdkAgents/errors.ts` (new)
- `src/main/sdkAgents/providers/LlmProvider.ts` (new)
- `src/main/db/preferences.ts` (add experimental flag key)
- `src/preload/index.d.ts` (extend window.api typing)

**Tasks:**

- [ ] Define `SdkSessionId` (branded string), `ConversationId` (UUIDv4), `MessageId` (UUIDv4).
- [ ] Define `SdkAgentEvent` as a `_tag`-discriminated union (see Patterns).
- [ ] Define `SdkAgentError` discriminated union in `errors.ts` with `_tag`: `auth_missing` | `auth_invalid` | `rate_limited` | `network` | `aborted` | `sdk_internal` | `profile_not_found` | `unknown`, plus `sdkAgentErrorMessage()` matcher.
- [ ] Define `LlmProvider` interface with `query(options) → AsyncIterable<SdkAgentEvent>` and `providerId: 'anthropic' | 'openai' | 'gemini'`.
- [ ] Define `ToolDecision = 'allow-once' | 'allow-session' | 'deny'` and `AskUserQuestionAnswer` + `PlanDecision = { action: 'approve' } | { action: 'reject', feedback?: string }`.
- [ ] Add `experimental_sdk_agents` boolean preference (default `false`) via the existing PreferencesStore migration list.

**Patterns:**

```ts
// src/main/sdkAgents/types.ts
export type SdkAgentEvent =
  | { _tag: 'session.init'; sessionId: ConversationId; sdkSessionId: string; model: string }
  | { _tag: 'assistant.delta'; sessionId: ConversationId; messageId: MessageId; text: string }
  | {
      _tag: 'assistant.message'
      sessionId: ConversationId
      messageId: MessageId
      content: ContentBlock[]
      tokensIn?: number
      tokensOut?: number
    }
  | {
      _tag: 'tool.start'
      sessionId: ConversationId
      toolEventId: string
      name: string
      input: Record<string, unknown>
    }
  | {
      _tag: 'tool.result'
      sessionId: ConversationId
      toolEventId: string
      result: string
      isError: boolean
      durationMs: number
    }
  | {
      _tag: 'tool.permission_request'
      sessionId: ConversationId
      requestId: string
      toolName: string
      input: Record<string, unknown>
    }
  | {
      _tag: 'ask_user_question'
      sessionId: ConversationId
      requestId: string
      questions: Question[]
    }
  | { _tag: 'plan_mode_exit'; sessionId: ConversationId; requestId: string; plan: string }
  | {
      _tag: 'subagent.start'
      sessionId: ConversationId
      subagentId: string
      task: string
      agentType: string
    }
  | { _tag: 'subagent.event'; sessionId: ConversationId; subagentId: string; event: SdkAgentEvent }
  | {
      _tag: 'subagent.end'
      sessionId: ConversationId
      subagentId: string
      summary?: string
      status: 'success' | 'error'
    }
  | {
      _tag: 'usage'
      sessionId: ConversationId
      inputTokens: number
      outputTokens: number
      costUsd?: number
    }
  | { _tag: 'error'; sessionId: ConversationId; error: SdkAgentError }
  | { _tag: 'session.end'; sessionId: ConversationId; reason: 'completed' | 'aborted' | 'error' }
```

**Notes:** Freeze this file early — the renderer store, IPC layer, and persistence mapper all read from it. Use `ts-pattern` `.exhaustive()` downstream so new event kinds force renderer updates.

---

### Phase 2 — Persistence: SQLite schema + stores

**Goal:** commit the DB schema. Every later phase writes/reads through these stores.

**Locations:**

- `src/main/db/Database.ts` (append migration 11 + FTS5 migration 12)
- `src/main/db/ConversationStore.ts` (new)
- `src/main/db/MessageStore.ts` (new)
- `src/main/db/ToolEventStore.ts` (new)
- `src/main/db/SdkAttachmentStore.ts` (new — metadata only; blobs are files on disk)

**Tasks:**

- [ ] Add migration 11: `conversations`, `messages`, `tool_events`, `sdk_attachments` tables.
- [ ] Add migration 12: FTS5 virtual table `messages_fts` over `messages(content)` with triggers to keep it in sync.
- [ ] Implement `ConversationStore` with `create`, `get`, `list(workspaceId)`, `rename`, `hardDelete`, `updateStatus`, `search(query)`.
- [ ] Implement `MessageStore` with `append`, `listByConversation`, `getLatest`, `updateTokenUsage`.
- [ ] Implement `ToolEventStore` with `start`, `complete`, `listByMessage`.
- [ ] Implement `SdkAttachmentStore` for file metadata rows; files themselves go under `app.getPath('userData')/attachments/<conversationId>/<attachmentId>.<ext>`.
- [ ] All stores return `Result<T, DbError>` (reuse existing `DbError` pattern).

**Patterns:**

```sql
-- migration 11
CREATE TABLE conversations (
  id TEXT PRIMARY KEY,                    -- Canopy UUIDv4
  workspace_id TEXT NOT NULL,
  worktree_path TEXT NOT NULL,
  agent_profile_id TEXT NOT NULL,
  sdk_session_id TEXT,                    -- captured from SDK session.init; nullable until first message
  title TEXT,                             -- auto-generated by Haiku; nullable until first assistant msg
  model TEXT NOT NULL,
  permission_mode TEXT NOT NULL,          -- snapshot at session start
  status TEXT NOT NULL DEFAULT 'active',  -- active | ended | error | cancelled
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
  FOREIGN KEY(agent_profile_id) REFERENCES agent_profiles(id) ON DELETE RESTRICT
);
CREATE INDEX idx_conv_workspace ON conversations(workspace_id, updated_at DESC);

CREATE TABLE messages (
  id TEXT PRIMARY KEY,                    -- UUIDv4
  conversation_id TEXT NOT NULL,
  role TEXT NOT NULL,                     -- user | assistant | system | tool
  content TEXT NOT NULL,                  -- searchable plaintext (rendered from ContentBlock[])
  content_json TEXT NOT NULL,             -- canonical ContentBlock[] JSON (verbatim from SDK)
  tool_calls_json TEXT,                   -- nullable
  tokens_in INTEGER,
  tokens_out INTEGER,
  created_at TEXT NOT NULL,
  FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);
CREATE INDEX idx_msg_conv ON messages(conversation_id, created_at);

CREATE TABLE tool_events (
  id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  input_json TEXT NOT NULL,
  result_text TEXT,
  is_error INTEGER NOT NULL DEFAULT 0,
  decision TEXT,                          -- allow-once | allow-session | deny | auto
  duration_ms INTEGER,
  created_at TEXT NOT NULL,
  FOREIGN KEY(message_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE TABLE sdk_attachments (
  id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  kind TEXT NOT NULL,                     -- image | text
  filename TEXT NOT NULL,
  path TEXT NOT NULL,                     -- absolute path under userData/attachments/
  mime_type TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY(message_id) REFERENCES messages(id) ON DELETE CASCADE
);

-- migration 12
CREATE VIRTUAL TABLE messages_fts USING fts5(
  content, conversation_id UNINDEXED, created_at UNINDEXED,
  content='messages', content_rowid='rowid'
);
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, content, conversation_id, created_at)
  VALUES (new.rowid, new.content, new.conversation_id, new.created_at);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.rowid, old.content);
END;
CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content) VALUES('delete', old.rowid, old.content);
  INSERT INTO messages_fts(rowid, content) VALUES(new.rowid, new.content);
END;
```

**Notes:**

- Extend `ProfilePrefs` in `src/main/profiles/types.ts` with `mcpServers?: string` (raw JSON) and update `LEGACY_PREF_FIELDS.claude` — but do it in Phase 3 or later if that's cleaner; the profiles migration step is separate from the conversations migration.
- FTS5 content is the plaintext rendering (`content` column). On inserts, derive `content` by walking ContentBlock[] and concatenating text blocks.
- Hard-delete cascades via FK — no soft-delete columns.

---

### Phase 3 — `SdkAgentManager` + `AnthropicProvider`

**Goal:** runnable agent sessions in main, emitting typed events, without yet touching the UI.

**Locations:**

- `src/main/sdkAgents/SdkAgentManager.ts` (new)
- `src/main/sdkAgents/providers/AnthropicProvider.ts` (new)
- `src/main/sdkAgents/sessionRegistry.ts` (new — in-memory `Map<ConversationId, ActiveSession>`)
- `src/main/sdkAgents/eventMapper.ts` (new — SDK message → `SdkAgentEvent`)
- `src/main/index.ts` (instantiate the manager at startup, pass to IPC registration)

**Tasks:**

- [ ] `SdkAgentManager` constructor takes `(db, profileStore, preferencesStore, logger)`. Holds `providers: Map<string, LlmProvider>` with `anthropic` registered.
- [ ] `createSession({ workspaceId, worktreeRoot, profileId }) → Result<{ conversationId }, SdkAgentError>`: writes the `conversations` row, instantiates an `ActiveSession` shell with `AbortController` + event emitter + pending-permission map.
- [ ] `sendMessage(conversationId, userMessage)`: looks up profile, decrypts API key, builds SDK options, calls `provider.query(...)`. Iterates the async generator, mapping SDK messages → `SdkAgentEvent` via `eventMapper.ts`, persisting each, and emitting to the session's listeners.
- [ ] Listener registration: `onEvent(conversationId, (ev) => void)` returns an unsubscribe. Main's IPC layer (Phase 4) will subscribe and relay to the renderer.
- [ ] `cancel(conversationId)`: aborts the AbortController, emits `session.end` with `reason: 'aborted'`.
- [ ] `closeSession(conversationId)`: abort + detach listeners + evict from registry.
- [ ] `AnthropicProvider.query()`: wraps `@anthropic-ai/claude-agent-sdk`'s `query()`. Passes `permissionMode`, `model`, `systemPrompt`, `tools`, `toolConfig.askUserQuestion.previewFormat = 'markdown'`, `mcpServers`, `cwd`, and a `canUseTool` callback that delegates to the session's pending-permission map.
- [ ] `canUseTool` routing: emit `tool.permission_request` / `ask_user_question` / `plan_mode_exit` event; await a promise registered in the session's pending map keyed by `requestId`; resolve the promise from Phase 4/7 UI responses; return the SDK-shaped `{ behavior: 'allow' | 'deny', ... }`.
- [ ] 429 / retry: wrap the `query()` iterator in a retry decorator — exponential backoff with `retry-after` header respect, 3 attempts.

**Patterns:**

```ts
// src/main/sdkAgents/SdkAgentManager.ts (shape sketch)
export class SdkAgentManager {
  private sessions = new Map<ConversationId, ActiveSession>()
  private providers = new Map<string, LlmProvider>([['anthropic', new AnthropicProvider(this.logger)]])

  async sendMessage(id: ConversationId, text: string, attachments: Attachment[]): Promise<Result<void, SdkAgentError>> {
    const session = this.sessions.get(id)
    if (!session) return err({ _tag: 'profile_not_found' })
    // persist user message row
    // for await (const ev of provider.query(...)) { persist + emit(ev) }
  }

  respondToPermission(id: ConversationId, requestId: string, decision: ToolDecision): void { ... }
  respondToQuestion(id: ConversationId, requestId: string, answers: Record<string, AskUserQuestionAnswer>): void { ... }
  respondToPlan(id: ConversationId, requestId: string, decision: PlanDecision): void { ... }
}
```

**Notes:**

- `AnthropicProvider` is the only file that imports `@anthropic-ai/claude-agent-sdk` (besides `commitMessageGenerator.ts`). Everything upstream is SDK-agnostic.
- Session permission decisions live in `session.sessionAllowedTools: Set<string>`. Before routing a new `tool.permission_request`, check if the tool is in the set; if so, auto-resolve `{ behavior: 'allow' }` without a UI prompt.
- Subagent events: the SDK surfaces `Task`-tool sub-agent runs as nested message streams; `eventMapper.ts` wraps those in `subagent.*` events keyed by a Canopy-generated `subagentId`.
- Logging: write per-session log lines to `~/.config/Canopy/logs/sdk/<conversationId>.log` with JSON entries; rotate at 7 days via a startup sweep.

---

### Phase 4 — IPC surface + preload API

**Goal:** expose the manager to the renderer via typed IPC.

**Locations:**

- `src/main/ipc/handlers.ts` (register new handlers via `registerSdkAgentHandlers`)
- `src/main/sdkAgents/ipcHandlers.ts` (new — handler definitions)
- `src/preload/index.ts` (expose `window.api.sdkAgent.*`)
- `src/preload/index.d.ts` (types)

**Tasks:**

- [ ] Channel: `sdkAgent:create` — `(workspaceId, worktreeRoot, profileId) → { conversationId }`.
- [ ] Channel: `sdkAgent:send` — `(conversationId, text, attachments) → void`.
- [ ] Channel: `sdkAgent:cancel` — `(conversationId) → void`.
- [ ] Channel: `sdkAgent:close` — `(conversationId) → void`.
- [ ] Channel: `sdkAgent:list` — `(workspaceId) → ConversationSummary[]`.
- [ ] Channel: `sdkAgent:getTranscript` — `(conversationId) → { messages, toolEvents }`.
- [ ] Channel: `sdkAgent:search` — `(workspaceId, query) → SearchHit[]` (FTS5).
- [ ] Channel: `sdkAgent:delete` — `(conversationId) → void`.
- [ ] Channel: `sdkAgent:respondPermission` — `(conversationId, requestId, decision)`.
- [ ] Channel: `sdkAgent:respondQuestion` — `(conversationId, requestId, answers)`.
- [ ] Channel: `sdkAgent:respondPlan` — `(conversationId, requestId, decision)`.
- [ ] Push event: `sdkAgent:event:<conversationId>` — serialized `SdkAgentEvent`. Registered via `sdkAgent:subscribe` / `sdkAgent:unsubscribe` so only open panes receive events.
- [ ] IPC handlers unwrap `Result` via `unwrapOrThrow()` for write ops and `.unwrapOr(defaultValue)` for reads, per CLAUDE.md.
- [ ] All handlers go through the central `registerIpcHandlers` dependency-injection pattern in `src/main/ipc/handlers.ts` — add `SdkAgentManager` to the dependency list.

**Patterns:**

```ts
// src/preload/index.ts
contextBridge.exposeInMainWorld('api', {
  ...existing,
  sdkAgent: {
    create: (args) => ipcRenderer.invoke('sdkAgent:create', args),
    send: (args) => ipcRenderer.invoke('sdkAgent:send', args),
    cancel: (conversationId) => ipcRenderer.invoke('sdkAgent:cancel', conversationId),
    subscribe: (conversationId, handler) => {
      const channel = `sdkAgent:event:${conversationId}`
      const listener = (_, ev) => handler(ev)
      ipcRenderer.on(channel, listener)
      return () => ipcRenderer.off(channel, listener)
    },
    respondPermission: (args) => ipcRenderer.invoke('sdkAgent:respondPermission', args),
    respondQuestion: (args) => ipcRenderer.invoke('sdkAgent:respondQuestion', args),
    respondPlan: (args) => ipcRenderer.invoke('sdkAgent:respondPlan', args),
    list: (workspaceId) => ipcRenderer.invoke('sdkAgent:list', workspaceId),
    getTranscript: (conversationId) => ipcRenderer.invoke('sdkAgent:getTranscript', conversationId),
    search: (args) => ipcRenderer.invoke('sdkAgent:search', args),
    delete: (conversationId) => ipcRenderer.invoke('sdkAgent:delete', conversationId),
    close: (conversationId) => ipcRenderer.invoke('sdkAgent:close', conversationId),
  },
})
```

**Notes:**

- Event channel is per-conversationId to avoid flooding panes that aren't listening.
- Serialize events via `structuredClone`-safe shape. No Date objects — ISO strings.
- Response channels (`respondPermission` etc.) resolve the pending promise inside `SdkAgentManager`. Missing / already-resolved `requestId` → no-op (logged warning).

---

### Phase 5 — Renderer store + session state

**Goal:** consume IPC events on the renderer side and keep a reactive per-conversation store the chat pane renders from.

**Locations:**

- `src/renderer/src/lib/stores/sdkAgentSessions.svelte.ts` (new)
- `src/renderer/src/lib/agents/agentState.svelte.ts` (extend to include SDK-session summaries for the inspector)

**Tasks:**

- [ ] `openConversation(conversationId)`: fetches transcript, subscribes to events, hydrates `messages`, `toolEvents`, `subagents`, `pendingAttentionBlocks` arrays.
- [ ] `closeConversation(conversationId)`: unsubscribes, retains last snapshot in memory for a brief idle window.
- [ ] Event reducer uses `ts-pattern` on `_tag`:
  - `session.init` → set `sdkSessionId`, mark `status: 'idle'`.
  - `assistant.delta` → append to the streaming assistant message (create if missing).
  - `assistant.message` → finalize the streaming message (token counts, final content).
  - `tool.start` / `tool.result` → append/update `toolEvents`.
  - `tool.permission_request` / `ask_user_question` / `plan_mode_exit` → push into `pendingAttentionBlocks`.
  - `subagent.*` → nested branch managed by a `SubagentAggregator` helper that reduces nested events into a tree of sub-agent sessions.
  - `error` → append `AssistantErrorBlock` marker.
  - `session.end` → set `status: 'ended' | 'error' | 'cancelled'`.
- [ ] Expose derived values for the pane: `messages`, `isStreaming`, `tokensTotal`, `costUsdEstimate`, `currentAttention`.
- [ ] Extend `agentState.svelte.ts` to expose a `sdkAgentSummaries` array for the inspector (one line per active SDK session in the workspace).

**Patterns:**

```ts
// shape sketch
export type SdkMessage = {
  id: MessageId
  role: 'user' | 'assistant' | 'system'
  content: string
  contentBlocks: ContentBlock[]
  tokensIn?: number
  tokensOut?: number
  toolEventIds: string[]
  attentionBlocks: AttentionBlock[] // inline ask / plan / permission cards
  status: 'streaming' | 'done' | 'cancelled' | 'error'
  createdAt: string
}

export type AttentionBlock =
  | {
      kind: 'question'
      requestId: string
      questions: Question[]
      status: 'waiting' | 'resolved' | 'cancelled'
      answers?: Record<string, AskUserQuestionAnswer>
    }
  | {
      kind: 'plan'
      requestId: string
      plan: string
      status: 'waiting' | 'approved' | 'rejected'
      feedback?: string
    }
  | {
      kind: 'permission'
      requestId: string
      toolName: string
      input: Record<string, unknown>
      status: 'waiting' | 'granted' | 'denied'
      decision?: ToolDecision
    }
```

**Notes:**

- Use a plain `$state` object keyed by `conversationId` rather than Svelte stores (`writable`) — rest of Canopy has standardized on `$state`-backed modules.
- Derived cost: `costUsd = (tokensIn / 1M) * inRate + (tokensOut / 1M) * outRate` where rates are looked up per-model from a small hardcoded table; clearly labelled "estimated".

---

### Phase 6 — New pane kind: `SdkChatPane` + `MessageStream`

**Goal:** render a full chat pane driven by the renderer store, using the existing chat atoms/molecules.

**Locations:**

- `src/renderer/src/components/chat/organisms/SdkChatPane.svelte` (new organism — this IS an organism per the earlier scope boundary, hence under `organisms/`)
- `src/renderer/src/components/chat/organisms/MessageStream.svelte` (new)
- `src/renderer/src/components/chat/organisms/ConversationListSidebar.svelte` (new — basic list of conversations for the current worktree)
- `src/renderer/src/components/chat/molecules/AssistantErrorBlock.svelte` (new molecule — the one gap in the existing molecule set)
- `src/renderer/src/lib/stores/tabs.svelte.ts` (register pane kind)
- `src/renderer/src/Root.svelte` or equivalent (mount logic when a tab's kind is `sdkChat`)

**Tasks:**

- [ ] Create the `organisms/` folder — it does not yet exist.
- [ ] `MessageStream.svelte`: consumes `messages` array, renders `MessageBubble`s. Within each message, renders `toolEvents` via `ToolCallBlock`, nested sub-agent runs via `SubAgentRun`, and attention blocks inline via `QuestionnaireBlock` / `PlanApprovalBlock` / `ToolPermissionBlock`.
- [ ] `SdkChatPane.svelte`: composes `MessageStream` + `ChatInput` + `SendControl` + `ModelPickerInline`. Subscribes to the session via `sdkAgentSessions` store. Handles ⌘. shortcut for cancel. Shows empty state when no messages.
- [ ] `AssistantErrorBlock.svelte`: reuses `AttentionBanner` with `tone: 'danger'`. Shows `_tag`-specific messaging via `ts-pattern`, plus a `Retry` button that re-sends the previous user message.
- [ ] `ConversationListSidebar.svelte`: lists this worktree's conversations. New-chat button creates a new session. Click → loads transcript in the current pane.
- [ ] `tabs.svelte.ts`: add `'sdkChat'` to the pane kind union. Factory function: `createSdkChatPane({ workspaceId, worktreeRoot, conversationId? })`.
- [ ] Gate the new pane kind behind `prefs.experimental_sdk_agents`. Command palette entry: "New SDK chat" hidden when flag is off.

**Patterns:**

```svelte
<!-- SdkChatPane.svelte (sketch) -->
<script lang="ts">
  import { getSdkSession } from '$lib/stores/sdkAgentSessions.svelte'
  import MessageStream from './MessageStream.svelte'
  import ChatInput from '../molecules/ChatInput.svelte'

  let { conversationId }: { conversationId: string } = $props()
  const session = getSdkSession(conversationId)

  function onSubmit(text: string, attachments: Attachment[]) {
    window.api.sdkAgent.send({ conversationId, text, attachments })
  }
</script>

<div class="chat-pane">
  <MessageStream messages={session.messages} />
  <ChatInput onsubmit={onSubmit} disabled={session.isStreaming && !session.canCancel} />
</div>
```

**Notes:**

- Per-message attention blocks render INSIDE the assistant `MessageBubble`'s body (not between bubbles) — mirrors how the demo renders them inline.
- Sub-agent events render as `SubAgentRun` blocks; the nested event stream is flat for MVP (no deep nesting).
- `MessageStream` auto-scrolls to the bottom when streaming and the user is near the bottom; pauses auto-scroll if the user scrolls up mid-stream (standard chat-UX pattern).

---

### Phase 7 — `canUseTool` flows wired end-to-end

**Goal:** user clicks Allow/Deny/Submit → SDK unblocks. This is the feature that justifies the whole effort.

**Locations:**

- `src/renderer/src/components/chat/organisms/MessageStream.svelte` (attention-block rendering)
- `src/main/sdkAgents/SdkAgentManager.ts` (pending-promise registry)
- `src/main/sdkAgents/ipcHandlers.ts` (respond\* handlers)
- `src/renderer/src/components/dialogs/ConfirmDialog.svelte` (reuse for pane-close mid-pending)
- `src/renderer/src/lib/stores/tabs.svelte.ts` (intercept close when pending)

**Tasks:**

- [ ] `SdkAgentManager.respondPermission / respondQuestion / respondPlan` resolve the pending promise keyed by `requestId`.
- [ ] Wire `MessageStream`'s `QuestionnaireBlock.onsubmit` → `window.api.sdkAgent.respondQuestion(...)`. Optimistically mark the block as `resolved` in the store; confirm on the next event from main.
- [ ] `PlanApprovalBlock.onapprove` → `respondPlan({ action: 'approve' })`; on approve, main calls `query.setPermissionMode('acceptEdits')` so subsequent tool calls auto-allow file edits.
- [ ] `PlanApprovalBlock.onreject(feedback)` → `respondPlan({ action: 'reject', feedback })`. Main returns `{ behavior: 'deny', message: feedback }` to the SDK and subsequently sends `feedback` as a new user message (SDK resume model).
- [ ] `ToolPermissionBlock.onrespond(decision)` → `respondPermission(...)`. On `allow-session`, main adds the tool name to `session.sessionAllowedTools` so future same-tool calls auto-pass.
- [ ] Pane-close guard: if `session.pendingAttentionBlocks.length > 0`, open the existing `ConfirmDialog` ("Abort or keep open?"). On abort, call `sdkAgent:cancel`; the manager resolves pending promises with `{ behavior: 'deny' }` and emits `session.end { reason: 'aborted' }`.

**Patterns:**

```ts
// main-side pending registry
interface ActiveSession {
  pending: Map<string, (v: ToolDecision | PlanDecision | AskUserQuestionAnswerMap) => void>
  sessionAllowedTools: Set<string>
  // ...
}

// canUseTool (AnthropicProvider) sketch
async function canUseTool(toolName, input, { signal, suggestions }) {
  if (toolName === 'AskUserQuestion') {
    const requestId = uuid()
    const answers = await waitPending(session, requestId, 'question', {
      questions: input.questions,
    })
    return { behavior: 'allow', updatedInput: { ...input, answers } }
  }
  if (toolName === 'ExitPlanMode') {
    const requestId = uuid()
    const decision = await waitPending(session, requestId, 'plan', { plan: input.plan })
    if (decision.action === 'approve') {
      return { behavior: 'allow', updatedInput: input }
    }
    return { behavior: 'deny', message: decision.feedback ?? 'User requested changes.' }
  }
  if (session.sessionAllowedTools.has(toolName)) return { behavior: 'allow', updatedInput: input }
  const requestId = uuid()
  const decision = await waitPending(session, requestId, 'permission', { toolName, input })
  if (decision === 'allow-session') session.sessionAllowedTools.add(toolName)
  if (decision === 'deny') return { behavior: 'deny', message: 'User denied.' }
  return { behavior: 'allow', updatedInput: input }
}
```

**Notes:**

- `waitPending` is a helper that stores `{resolve, reject}` keyed by `requestId`, emits the corresponding event (`tool.permission_request` / `ask_user_question` / `plan_mode_exit`), and returns the promise.
- On `session.cancel()`, sweep the pending map, resolving each with a deny/cancel so the SDK's canUseTool doesn't hang.
- `suggestions` from SDK ignored for MVP (per answers).

---

### Phase 8 — Sub-agents + MCP + inspector integration

**Goal:** sub-agent (Task-tool) runs render nicely; MCP servers configurable per profile; inspector shows SDK sessions.

**Locations:**

- `src/main/sdkAgents/SubagentAggregator.ts` (new)
- `src/main/profiles/types.ts` + `src/main/profiles/ProfileStore.ts` (extend `ProfilePrefs` with `mcpServers`)
- `src/renderer/src/components/profiles/ClaudeExtras.svelte` or a new `ClaudeSdkExtras.svelte` (add MCP-server editor)
- `src/renderer/src/components/agents/AgentInspector.svelte` (render SDK summaries)
- `src/renderer/src/components/agents/SdkSessionExtras.svelte` (new — inspector panel content for SDK sessions)

**Tasks:**

- [ ] Extend `ProfilePrefs` with `mcpServers?: string` (raw JSON validated at write time). Update `LEGACY_PREF_FIELDS.claude` to include the new key.
- [ ] Extend `ProfileStore` write path to parse/validate the JSON and surface a typed error.
- [ ] `AnthropicProvider.query()` parses `profile.prefs.mcpServers` and passes to SDK's `mcpServers` option.
- [ ] `SubagentAggregator`: SDK emits `Task`-tool events with nested message streams; the aggregator maps them to `subagent.*` events in our union. Renderer stores build a `subagents: Map<string, SdkMessage[]>` keyed by subagentId.
- [ ] Decision: reuse `AgentType = 'claude'` for SDK sessions OR introduce `'claude-sdk'`. **Recommendation for the spec:** introduce `'claude-sdk'` as a new AgentType — cleaner prefs ownership (MCP, `tools`, advanced fields), independent default profile, no migration collisions. `KNOWN_AGENT_TYPES` becomes `['claude', 'claude-sdk', 'gemini', 'opencode', 'codex']`.
- [ ] `AgentInspector` switches on `agentType`. `'claude-sdk'` renders `SdkSessionExtras` with tokens, cost, streaming state, `Stop` button.
- [ ] Inspector session list aggregates PTY sessions + SDK sessions from `agentState` + `sdkAgentSessions`; SDK rows show a small `<SubAgentBadge type="SDK" />`.

**Patterns:**

```ts
// ProfilePrefs extension
export interface ProfilePrefs {
  // ... existing
  mcpServers?: string // raw JSON; shape: Record<string, { command: string, args: string[], env?: Record<string, string> }>
}
```

**Notes:**

- MCP validation: parse JSON → shape-check with `ts-pattern` or a light runtime checker; reject invalid configs with a typed error.
- Sub-agent events: SDK v0.2.100's exact event shape for Task tool nested streams needs verification at implementation time — if the SDK doesn't surface nested streams, render sub-agent runs as ordinary tool-call blocks (fallback path already handled by `ToolCallBlock`).

---

### Phase 9 — Attachments, slash commands, streaming polish

**Goal:** user can paste images, attach text files, type `/model` in the input, and see smooth token streaming.

**Locations:**

- `src/renderer/src/components/chat/organisms/SdkChatPane.svelte` (paste / drop handlers)
- `src/renderer/src/components/chat/molecules/ChatInput.svelte` (slash-command hint popover — reuses existing `SlashCommandHint`)
- `src/main/sdkAgents/attachmentPipeline.ts` (new — copy-to-userData + read-back)
- `src/main/db/SdkAttachmentStore.ts` (from Phase 2)

**Tasks:**

- [ ] Input: accept drag-dropped files + pasted images. For each, call `sdkAgent:uploadAttachment(conversationId, file)` — IPC copies the file to `userData/attachments/<conversationId>/<id>.<ext>`, creates the DB row, returns `{ id, filename, kind }`.
- [ ] Size cap: images ≤ 5 MB, text ≤ 200 KB. Exceed → toast + reject.
- [ ] On send, the user message carries `attachmentIds: string[]`. Main reads attachment bytes for images → base64 → `ContentBlock` `{ type: 'image', source: { type: 'base64', ... } }`. Text files → append as fenced code block to the user's text.
- [ ] Slash commands: parsed at send-time by `SdkChatPane`:
  - `/new` — create a new conversation in the same pane.
  - `/clear` — clear the display but keep the transcript (UI-level only; DB untouched).
  - `/model <name>` — override model for the next message.
  - `/mode <default|plan|acceptEdits|bypassPermissions>` — change permission mode for the next message via `query.setPermissionMode()`.
  - `/retry` — resend the previous user message.
  - Commands not matched are sent as plain text.
- [ ] Streaming polish: `assistant.delta` events accumulated into the current streaming message; throttle UI updates to 60 fps via `requestAnimationFrame` batching.
- [ ] Auto-scroll logic: scroll to bottom when near-bottom (within 80 px). Pause when user scrolls up; a floating "Scroll to latest" button appears to resume.

**Patterns:**

```ts
// attachment copy (main)
async function uploadAttachment(conversationId: ConversationId, buffer: Buffer, meta: {...}) {
  const id = uuid()
  const ext = path.extname(meta.filename)
  const attachmentsDir = path.join(app.getPath('userData'), 'attachments', conversationId)
  await fs.promises.mkdir(attachmentsDir, { recursive: true })
  const target = path.join(attachmentsDir, `${id}${ext}`)
  await fs.promises.writeFile(target, buffer)
  attachmentStore.insert({ id, conversationId, /* ... */ path: target })
  return { id, path: target, filename: meta.filename, kind: meta.kind }
}
```

**Notes:**

- Delete-conversation hard-deletes attachment files too — `ConversationStore.hardDelete` iterates `sdk_attachments` rows, unlinks the files, then deletes the conversation row (cascade removes the attachment rows).
- Streaming over IPC: `assistant.delta` events can be frequent (tens per second). The event bus should debounce at the transport if necessary, but the renderer's rAF batching is the primary throttle.

---

### Phase 10 — Error handling, retries, logging, FTS5 search UI, feature-flag flip

**Goal:** production-grade defensive behavior + the history sidebar search UI.

**Locations:**

- `src/main/sdkAgents/retry.ts` (new)
- `src/main/sdkAgents/sessionLog.ts` (new — per-session file log with 7-day sweep)
- `src/renderer/src/components/chat/organisms/ConversationListSidebar.svelte` (add FTS5 search input)
- `src/renderer/src/components/chat/molecules/AssistantErrorBlock.svelte` (from Phase 6 — finalize Retry behavior)
- `src/renderer/src/components/preferences/ExperimentalPane.svelte` or equivalent (add a toggle to flip `experimental_sdk_agents`)

**Tasks:**

- [ ] Retry decorator wraps `provider.query()`. On 429 / 5xx: up to 3 attempts, 1s / 4s / 16s backoff, respect `retry-after` header. Other errors: bubble out immediately.
- [ ] Error mapper: SDK errors → `SdkAgentError._tag` union. `AssistantErrorBlock` uses `ts-pattern` on `_tag` for copy.
- [ ] Retry button on `AssistantErrorBlock`: calls `sdkAgent:send` with the previous user message (store keeps a reference).
- [ ] Per-session log: JSON-line file opened on session create, one line per event, closed on session end. Startup task scans `~/.config/Canopy/logs/sdk/` and deletes files older than 7 days.
- [ ] Search UI: input above the conversation list; as user types, call `sdkAgent:search`; render hits with conversation title + snippet (FTS5 `snippet()` function). Enter navigates.
- [ ] Preferences toggle: hidden from preferences panel until the flag category "Experimental" is opened. Toggle ON reveals the new pane kind in the command palette + new-pane picker.

**Patterns:**

```ts
// retry wrapper sketch
async function* withRetry(
  iter: AsyncIterable<SdkAgentEvent>,
  maxAttempts = 3,
): AsyncIterable<SdkAgentEvent> {
  let attempt = 0
  while (true) {
    try {
      yield* iter
      return
    } catch (e) {
      const retryable = isRetryable(e)
      if (!retryable || attempt >= maxAttempts) throw e
      const delay = computeBackoff(e, attempt)
      await sleep(delay)
      attempt++
    }
  }
}
```

**Notes:**

- Logs MUST NOT contain raw prompts or API keys — redact at the logger level. Log `promptHash` (sha256 prefix) instead of `prompt`.
- `ConfirmDialog` for hard-delete (per answers) shows conversation title + message count.
- Feature-flag OFF by default. Doc in `docs/features/sdk-agents.md` (new) with a one-paragraph user-facing summary.

---

## Final Verification

This is presentational + runtime combined, so verification has three layers:

### 1. Static checks (run after every phase)

- `npm run svelte-check` → 0 errors, 0 warnings.
- `npm run typecheck` → passes (both node + svelte configs).
- `npm run lint` → no new errors (the pre-existing `jira.ts` prettier warning is OK).

### 2. Manual end-to-end smoke (run after Phase 7; re-run after Phases 8–10)

- With the feature flag ON: open a workspace, create an SDK chat pane. A fresh empty pane with input focus appears.
- Type "What's in this repo?" and send. Assistant message streams in; `MessageMeta` shows token counts after completion.
- Ask a clarifying question likely to trigger `AskUserQuestion`: "Help me pick a router for this app." → `QuestionnaireBlock` appears inline; answer; the conversation continues using the answer.
- Force a plan-mode flow: set permission mode to `plan` via `/mode plan`, ask "Refactor the auth module." → `PlanApprovalBlock` appears. Click Approve → permission flips to `acceptEdits`, Edit tools execute without prompting. Click Reject with feedback → the feedback posts as a new user message.
- Trigger a tool-permission prompt: ask "Delete the temp directory" (Bash) in `default` mode → `ToolPermissionBlock` with `Allow once / Allow for session / Deny`. Pick `Allow for session`. Ask again → no second prompt.
- Kill the pane while a permission prompt is pending → `ConfirmDialog` appears. Abort → session cancelled, transcript marks the attention block as `cancelled`.
- Quit and relaunch the app → pane list shows the conversation (history persisted). Open it → transcript renders read-only. Send a new message → SDK resumes the session.
- Search: type a phrase from a prior conversation → the sidebar FTS5 hit appears; click → jumps.
- Image attachment: paste an image → chip appears → send → the agent references the image in its reply (model must support vision).
- Force a 429 (via mocked provider) → retry banner doesn't show for the first 3 auto-retries; on final failure, `AssistantErrorBlock` appears with Retry button.
- Turn the feature flag OFF → the pane kind is not in the new-pane picker; command palette entry hidden; existing conversations no longer visible in the sidebar (but rows remain in DB).

### 3. Database integrity

- Open the SQLite DB with `sqlite3 ~/.config/Canopy/canopy.db`: verify `conversations`, `messages`, `tool_events`, `sdk_attachments`, `messages_fts` exist with expected columns.
- Run `SELECT COUNT(*) FROM messages_fts WHERE content MATCH 'router'` and confirm it matches the sidebar search output.
- Hard-delete a conversation → verify cascade: no orphan rows in `messages`, `tool_events`, `sdk_attachments`; attachment files on disk deleted.
- Attempt to delete an `agent_profiles` row that has active conversations → FK RESTRICT blocks it.
