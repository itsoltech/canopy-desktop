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

## Configuration

Preferences are per-agent and read from the preferences store via the adapter's `buildCliArgs()` and `buildEnvVars()` methods.

| Preference key              | Agent    | Purpose                                           |
| --------------------------- | -------- | ------------------------------------------------- |
| `claude.model`              | Claude   | `--model` argument                                |
| `claude.permissionMode`     | Claude   | `--permission-mode` argument                      |
| `claude.effortLevel`        | Claude   | `--effort` argument                               |
| `claude.appendSystemPrompt` | Claude   | `--append-system-prompt` argument                 |
| `claude.apiKey`             | Claude   | `ANTHROPIC_API_KEY` env var                       |
| `claude.baseUrl`            | Claude   | `ANTHROPIC_BASE_URL` env var                      |
| `claude.provider`           | Claude   | Sets `CLAUDE_CODE_USE_BEDROCK`/`VERTEX`/`FOUNDRY` |
| `claude.customEnv`          | Claude   | JSON object of additional env vars                |
| `codex.model`               | Codex    | `--model` argument                                |
| `codex.approvalMode`        | Codex    | `--ask-for-approval` argument                     |
| `codex.sandbox`             | Codex    | `--sandbox` argument                              |
| `codex.fullAuto`            | Codex    | `--full-auto` flag (when `"true"`)                |
| `codex.profile`             | Codex    | `--profile` argument                              |
| `codex.apiKey`              | Codex    | `OPENAI_API_KEY` env var                          |
| `codex.baseUrl`             | Codex    | `OPENAI_BASE_URL` env var                         |
| `codex.customEnv`           | Codex    | JSON object of additional env vars                |
| `gemini.model`              | Gemini   | `--model` argument                                |
| `gemini.approvalMode`       | Gemini   | `--approval-mode` argument                        |
| `gemini.apiKey`             | Gemini   | `GEMINI_API_KEY` env var                          |
| `gemini.customEnv`          | Gemini   | JSON object of additional env vars                |
| `opencode.model`            | OpenCode | `--model` argument                                |
| `opencode.apiKey`           | OpenCode | `ANTHROPIC_API_KEY` env var                       |
| `opencode.settingsJson`     | OpenCode | `OPENCODE_CONFIG_CONTENT` env var                 |
| `opencode.customEnv`        | OpenCode | JSON object of additional env vars                |

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
- Store: `src/renderer/src/lib/agents/agentState.svelte.ts`
- Components: `src/renderer/src/lib/agents/`
  - `worktreeStatus.svelte.ts` - aggregate agent status per worktree
