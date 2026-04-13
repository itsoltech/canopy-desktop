# AI assistant adapters

> Run AI coding assistants (Claude Code, Codex, Gemini CLI, OpenCode) inside Canopy with normalized session tracking and hook-based event integration.

**Status:** Stable
**Introduced:** v0.10.0
**Platforms:** All

## Overview

Canopy can spawn AI coding assistants as tools running inside PTY sessions. Each supported agent (Claude Code, Codex, Gemini CLI, OpenCode) has a dedicated adapter that configures hook scripts, normalizes lifecycle events into a common schema, and tracks session state (idle, thinking, tool calling, waiting for permission, error, ended).

The adapter system uses a shared HTTP server (`AgentHookRouter`) that listens on `127.0.0.1` and routes hook events and status updates to the correct session by URL path. Each session gets a unique auth token validated via `X-Canopy-Auth` header with timing-safe comparison. Shell scripts (`canopy-agent-hook.sh`, `canopy-agent-statusline.sh`) are injected as hook commands into each agent's configuration.

Session state is tracked in the renderer via `agentSessions`, a reactive record keyed by PTY session ID. The UI displays the agent's current status, model name, context window usage, cost, tool call count, active subagents, task lists, and notifications. A badge system (`none`, `unread`, `permission`) surfaces attention-needed states at both the tab and worktree levels.

## Behavior

### Spawning an agent session

1. User creates a new agent tab (or the system spawns one for a worktree).
2. `AgentSessionManager.createSession()` is called with the tool ID, worktree path, workspace name, and branch.
3. The adapter is looked up from the registry by `toolId` (`claude`, `codex`, `gemini`, `opencode`).
4. A new session is registered on the `AgentHookRouter`. The router allocates a per-session auth token and returns the server port and URL path (`/session/{sessionId}/hook` and `/session/{sessionId}/status`).
5. The adapter's `setupSettings()` creates the agent-specific configuration file with hook commands pointing to the Canopy shell scripts. Environment variables `CANOPY_HOOK_PORT` and `CANOPY_HOOK_TOKEN` are set so the scripts know where to POST events.
6. The agent process is spawned in a PTY with the adapter's CLI args and env vars injected.
7. The renderer initializes an `AgentSessionState` with status `inactive`.

### Agent-specific setup

**Claude Code:** Writes a temporary `settings.json` at `{userData}/canopy/agent-hooks/session-{uuid}.json` with hooks for 16 event types and an optional `statusLine` command. Passes `--settings {path}` to the CLI. Supports `--model`, `--permission-mode`, `--effort`, `--append-system-prompt` from preferences. Env vars: `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`, provider flags (`CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, `CLAUDE_CODE_USE_FOUNDRY`), and arbitrary custom env vars (with blocklist filtering).

**Codex:** Writes hooks to `.codex/hooks.json` inside the worktree directory. Adds `.codex/` to `.gitignore` if not already present. Uses refcounting for concurrent sessions sharing the same worktree. On cleanup, restores the original `hooks.json` content (or removes the file/directory if Canopy created it). Passes `--enable codex_hooks` plus `--model`, `--ask-for-approval`, `--sandbox`, `--full-auto`, `--profile` from preferences. Env vars: `OPENAI_API_KEY`, `OPENAI_BASE_URL`, custom env.

**Gemini CLI:** Creates an isolated home directory (`gemini-home-{uuid}`) with a `.gemini/` subdirectory. Symlinks user config files from `~/.gemini/` (except `settings.json`). Deep-merges Canopy hooks into the user's settings. Sets `GEMINI_CLI_HOME` to the isolated directory. Passes `--model`, `--approval-mode` from preferences. Env vars: `GEMINI_API_KEY`, custom env.

**OpenCode:** Creates a per-session config directory (`opencode-config-{uuid}`) with a `plugins/` subdirectory containing the `canopy-bridge.ts` plugin file. Sets `OPENCODE_CONFIG_DIR` to this directory (additive search path alongside `~/.config/opencode/`). Passes `--model` from preferences. Env vars: `ANTHROPIC_API_KEY`, `OPENCODE_CONFIG_CONTENT` for config overrides, custom env.

### Event normalization

Each agent emits events in its own protocol. Adapters map these to a common set of `NormalizedEventName` values:

| Normalized event      | Claude Code          | Codex              | Gemini CLI                     | OpenCode            |
| --------------------- | -------------------- | ------------------ | ------------------------------ | ------------------- |
| `SessionStart`        | `SessionStart`       | `SessionStart`     | `SessionStart`                 | `SessionCreated`    |
| `SessionEnd`          | `SessionEnd`         | -                  | `SessionEnd`                   | `SessionDeleted`    |
| `PromptSubmit`        | `UserPromptSubmit`   | `UserPromptSubmit` | `BeforeAgent`                  | `SessionBusy`       |
| `BeforeToolUse`       | `PreToolUse`         | `PreToolUse`       | `BeforeTool`                   | `ToolExecuteBefore` |
| `AfterToolUse`        | `PostToolUse`        | `PostToolUse`      | `AfterTool`                    | `ToolExecuteAfter`  |
| `PermissionRequest`   | `PermissionRequest`  | -                  | `Notification(ToolPermission)` | `PermissionAsked`   |
| `Idle`                | `Stop`               | `Stop`             | `AfterAgent`                   | `SessionStatusIdle` |
| `IdleFailure`         | `StopFailure`        | -                  | -                              | `SessionError`      |
| `BeforeCompact`       | `PreCompact`         | -                  | `PreCompress`                  | `SessionCompacting` |
| `AfterCompact`        | `PostCompact`        | -                  | -                              | `SessionCompacted`  |
| `Notification`        | `Notification`       | -                  | `Notification`                 | `TodoUpdated`       |
| `AfterToolUseFailure` | `PostToolUseFailure` | -                  | -                              | -                   |
| `SubagentStart`       | `SubagentStart`      | -                  | -                              | -                   |
| `SubagentStop`        | `SubagentStop`       | -                  | -                              | -                   |
| `TaskCompleted`       | `TaskCompleted`      | -                  | -                              | -                   |
| `TeammateIdle`        | `TeammateIdle`       | -                  | -                              | -                   |

Events that do not map to a known name are normalized as `Unknown`.

### Session state tracking

The renderer maintains per-session state in `agentSessions[ptySessionId]`:

- `status`: Discriminated union with types `inactive`, `starting`, `idle`, `thinking`, `compacting`, `toolCalling`, `waitingPermission`, `error`, `ended`.
- `model` / `modelId`: Model name and ID (updated from status line data or hook events).
- `contextPercent` / `contextSize`: Context window usage. Claude provides this via the status line. Gemini calculates it from `AfterModel.usageMetadata.totalTokenCount` divided by a lookup table of model context limits (`resources/gemini-models.json`).
- `costUsd` / `durationMs` / `linesAdded` / `linesRemoved`: Cost tracking (Claude only via status line).
- `tasks`: Task list populated from `TaskCreate`/`TaskUpdate` tool calls (Claude, Codex) or `TodoUpdated` events (OpenCode). Capped at 50 tasks; oldest completed tasks are evicted first.
- `notifications`: Rolling buffer of 20 notification events.
- `activeSubagents`: Tracked via `SubagentStart`/`SubagentStop` events (Claude).
- `compactCount` / `toolCallCount`: Counters incremented on relevant events.
- `extra`: Agent-specific data (Claude rate limits, Codex `cwd`/`transcriptPath`/`turnId`, OpenCode pending questions).

### Busy/idle tracking

Each adapter declares `busyEvents` and `idleEvents` sets. The `AgentSessionManager` tracks busy state per session so the notch overlay and other UI elements can reflect whether the agent is actively working.

### Notifications

When an adapter's `formatNotification()` returns a non-null value, a native OS notification is shown. Clicking the notification focuses the owner window and sends `agent:focusSession` to switch to the agent's tab. Currently, only `PermissionRequest` events trigger notifications (Claude, Gemini, OpenCode). Codex does not emit permission events. OpenCode treats the `question` tool call as a permission request, surfacing it in the same notification flow.

### Session context injection

On `SessionStart`, if the adapter implements `buildSessionContext()`, the hook response includes `additionalContext` describing the workspace name, worktree/branch, and project root path. All four adapters provide this context.

### Session resume

All four adapters support resuming a previous session:

- Claude: `--resume {sessionId}`
- Codex: `resume {sessionId}`
- Gemini: `--resume {sessionId}`
- OpenCode: `--continue --session {sessionId}`

### Worktree-level status aggregation

`getWorktreeAgentStatus()` scans all agent panes in a worktree's tabs and returns the highest-priority status: `waitingPermission` > `error` > `working` (thinking/toolCalling/compacting) > `idle` > `none`.

### Badge system

Badges indicate attention state at two levels:

- **Agent tab badge**: `none`, `unread`, or `permission`. Set by the event handler based on agent activity.
- **Worktree badge**: Aggregated from agent badges. A `permission` badge is never downgraded to `unread`.

### Cleanup

On session destroy, the adapter's `cleanup()` function removes temporary settings files, isolated home directories, and restores modified project files (Codex's `.codex/hooks.json` and `.gitignore`). `cleanupOrphans()` runs at startup to remove stale session files from the hooks directory.

### Profiles

Each agent can have multiple named **profiles**, each holding a complete configuration snapshot (model, API key, base URL, provider, env vars, settings JSON override). Profiles let users switch between providers — e.g. a `Default` profile using Anthropic, an `Ollama` profile pointing at a local endpoint, a `GLM` or `MinMax` profile targeting alternative gateways — without rewriting global preferences each time.

**Launching a profile.** The Tools sidebar renders each AI agent as a collapsible group when it has two or more profiles. Expanding the group lists the profiles; clicking one spawns the agent using that profile's configuration. When an agent has only a single profile (typically the `Default`), it renders as a flat launcher with no chevron — one click launches directly. If `profileId` is omitted from the `tool:spawn` payload, the spawn handler falls back to reading global preferences (legacy behaviour).

**Profile → adapter seam.** Adapters are profile-agnostic: they take a `PreferencesReader` interface (`{ get(key): string | null }`). When `tool:spawn` receives a `profileId`, it wraps the profile in a `profileToReader()` shim that returns the profile's values for `${agentType}.*` keys and delegates all other keys to the global `preferencesStore`. This means adding profile support required zero changes to `AgentSessionManager` or any of the four adapter files. All three reader call sites in `tool:spawn` (settingsJson parsing, `getCliArgs`, `getEnvVars`) swap to the shim together.

**Storage.** Profiles live in the `agent_profiles` SQLite table with columns `id`, `agent_type`, `name`, `is_default`, `sort_index`, `prefs_json`, `api_key_enc`, `created_at`, `updated_at`. `api_key_enc` is encrypted with Electron's `safeStorage` (identical pattern to `CredentialStore`; falls back to plain base64 on Linux without a keyring). The name is unique per agent type. Only a single profile per agent type may be deleted down to — the store returns `ProfileLastDeletion` if the user tries to remove the last profile.

**Migration.** On first launch after the feature lands, `ProfileStore.ensureDefaults()` runs (inside `app.whenReady()`, after `safeStorage` is initialized) and — for each agent type with zero profiles — reads the legacy `${agentType}.*` keys from the `preferences` table and inserts a `Default` profile with those values. The legacy rows are left in place so downgrades remain safe. The migration is idempotent: it runs on every startup but is a no-op once profiles exist.

**API key masking.** Profiles cross IPC as `AgentProfileMasked`, which replaces the decrypted `apiKey` with a boolean `hasApiKey`. The renderer never sees the decrypted key. On save, the renderer sends `apiKey: undefined` to keep the existing key, `apiKey: null` to clear it, or a new string to overwrite. The main process reads the decrypted key only via `ProfileStore.getInternal()`, which is never exposed over IPC.

**Layout restore.** `PaneSession.profileId` is serialized with tab layouts so restored tabs re-spawn with the same profile. The tab display name appends the profile name when it is not `"Default"` (e.g. `Claude Code (Ollama)`).

## Configuration

Preferences for each agent are organized into **profiles** (see the Profiles section above). The per-agent configuration UI lives at Settings → Claude / Gemini / OpenCode / Codex and renders a two-pane list-and-form editor backed by `AgentProfilesPanel.svelte`. Existing global preferences from earlier versions are migrated into a `Default` profile automatically.

The fields below describe the keys stored inside each profile's `prefs_json` (non-secret) and the separately encrypted `api_key_enc` column. Adapters read them via `${agentType}.<field>` lookups on the `PreferencesReader` shim.

| Profile field          | Agent    | Purpose                                           |
| ---------------------- | -------- | ------------------------------------------------- |
| `model`                | Claude   | `--model` argument                                |
| `permissionMode`       | Claude   | `--permission-mode` argument                      |
| `effortLevel`          | Claude   | `--effort` argument                               |
| `appendSystemPrompt`   | Claude   | `--append-system-prompt` argument                 |
| `apiKey` _(encrypted)_ | Claude   | `ANTHROPIC_API_KEY` env var                       |
| `baseUrl`              | Claude   | `ANTHROPIC_BASE_URL` env var                      |
| `provider`             | Claude   | Sets `CLAUDE_CODE_USE_BEDROCK`/`VERTEX`/`FOUNDRY` |
| `customEnv`            | Claude   | JSON object of additional env vars                |
| `settingsJson`         | Claude   | Merged into per-session `settings.json`           |
| `model`                | Codex    | `--model` argument                                |
| `approvalMode`         | Codex    | `--ask-for-approval` argument                     |
| `sandbox`              | Codex    | `--sandbox` argument                              |
| `fullAuto`             | Codex    | `--full-auto` flag (when `"true"`)                |
| `profile`              | Codex    | `--profile` argument                              |
| `apiKey` _(encrypted)_ | Codex    | `OPENAI_API_KEY` env var                          |
| `baseUrl`              | Codex    | `OPENAI_BASE_URL` env var                         |
| `customEnv`            | Codex    | JSON object of additional env vars                |
| `settingsJson`         | Codex    | Merged into per-session `.codex/hooks.json`       |
| `model`                | Gemini   | `--model` argument                                |
| `approvalMode`         | Gemini   | `--approval-mode` argument                        |
| `apiKey` _(encrypted)_ | Gemini   | `GEMINI_API_KEY` env var                          |
| `customEnv`            | Gemini   | JSON object of additional env vars                |
| `settingsJson`         | Gemini   | Merged into per-session `.gemini/settings.json`   |
| `model`                | OpenCode | `--model` argument                                |
| `apiKey` _(encrypted)_ | OpenCode | `ANTHROPIC_API_KEY` env var                       |
| `settingsJson`         | OpenCode | `OPENCODE_CONFIG_CONTENT` env var                 |
| `customEnv`            | OpenCode | JSON object of additional env vars                |

Custom env vars are filtered against a blocklist (`BLOCKED_ENV_VARS` from `security/envBlocklist`) and internal vars (`CANOPY_HOOK_PORT`, `CANOPY_HOOK_TOKEN`, `ELECTRON_RUN_AS_NODE`).

## Error states

Agent errors surface through the normalized event system rather than a dedicated error type.

| Condition                  | User sees                                               | Cause                                                              |
| -------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------ |
| No adapter for tool        | "No agent adapter for tool: {toolId}"                   | Tool ID not registered in the adapter registry                     |
| `IdleFailure` event        | Error badge on tab; status shows error type and details | Agent process hit an unrecoverable error (API auth failure, crash) |
| `SessionEnd` event         | Status shows "ended" with reason                        | Agent process exited (user quit, context exhausted, error)         |
| Hook server body too large | Event silently dropped                                  | Hook payload exceeds 1 MB limit                                    |
| Hook auth mismatch         | 403 response to agent                                   | Stale or invalid `X-Canopy-Auth` token                             |

## Security and privacy

- The hook server binds to `127.0.0.1` only (no network exposure).
- Each session has a unique 256-bit auth token validated with `timingSafeEqual`.
- Hook request bodies are capped at 1 MB.
- API keys set via preferences are injected as environment variables, not written to settings files.
- Custom env vars are filtered against a blocklist that includes sensitive Electron internals.
- Codex's `.codex/hooks.json` (which contains local filesystem paths) is automatically added to `.gitignore`.
- Gemini sessions run in isolated home directories so concurrent sessions do not interfere.
- Credential autofill for the browser (separate feature) uses an isolated JavaScript world to prevent page script interception.
- **Profile API key storage:** profile API keys are encrypted with Electron's `safeStorage`, which delegates to the OS keychain (Keychain on macOS, DPAPI on Windows, libsecret/kwallet on Linux). On Linux without a running keyring daemon, `safeStorage.isEncryptionAvailable()` returns false and the keys fall back to plain base64 in the SQLite file — **base64 is encoding, not encryption**, and the keys are trivially recoverable from `canopy.db`. A console warning is logged when this happens. To get OS-level encryption on Linux, install and start `gnome-keyring` or `kwallet` before launching Canopy. The same fallback applies to existing encrypted preference keys (`*.apiKey`, task tracker tokens) and to `CredentialStore`; this is not new behaviour, but profile users should be aware.
- **Custom env var values** entered in the profile editor are masked by default in the UI (rendered as dots) and can be revealed per-row via a "Show" button. They are still stored in plain text inside the profile's `prefs_json` blob — they are not treated as secrets at the storage layer, so do not paste keys into the env vars field expecting encryption. Use the dedicated **API key** field for that.

## Source files

- Main: `src/main/agents/`
  - `AgentSessionManager.ts` - session lifecycle, adapter registration, notification dispatch
  - `AgentHookServer.ts` - shared HTTP server routing hook/status events by session
  - `registry.ts` - adapter registry (register, lookup, enumerate)
  - `types.ts` - `AgentAdapter` interface, `NormalizedHookEvent`, `NormalizedStatusData`
  - `utils.ts` - deep merge, tool input summarization
  - `adapters/claude.ts` - Claude Code adapter
  - `adapters/codex.ts` - Codex adapter
  - `adapters/gemini.ts` - Gemini CLI adapter
  - `adapters/opencode.ts` - OpenCode adapter
- Profiles: `src/main/profiles/`
  - `types.ts` - `AgentProfile`, `AgentProfileMasked`, `ProfilePrefs`, `ProfileInput`, `LEGACY_PREF_FIELDS`
  - `errors.ts` - `ProfileError` tagged union with `ts-pattern.exhaustive()` formatter
  - `ProfileStore.ts` - SQLite CRUD, `ensureDefaults()` migration, `profileToReader()` shim
- Store: `src/renderer/src/lib/agents/agentState.svelte.ts`
- Components: `src/renderer/src/lib/agents/`
  - `worktreeStatus.svelte.ts` - aggregate agent status per worktree
- Renderer (profiles UI):
  - `src/renderer/src/lib/stores/profiles.svelte.ts` - reactive profile list + CRUD actions
  - `src/renderer/src/components/preferences/AgentProfilesPanel.svelte` - two-pane list + form editor
  - `src/renderer/src/components/preferences/ProfileEnvVarsSection.svelte` - shared env-vars editor
  - `src/renderer/src/components/preferences/{Claude,Gemini,OpenCode,Codex}ProfileForm.svelte` - per-agent field grids
  - `src/renderer/src/components/sidebar/ToolSection.svelte` - expandable AI groups, flat single-profile launchers
