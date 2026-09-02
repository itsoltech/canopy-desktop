# AI assistant adapters

> Run AI coding assistants (Claude Code, Codex, Gemini CLI, OpenCode) inside Canopy with normalized session tracking and hook-based event integration.

**Status:** Stable
**Introduced:** v0.10.0
**Platforms:** All

## Overview

Canopy can spawn AI coding assistants as tools running inside PTY sessions. Each supported agent (Claude Code, Codex, Gemini CLI, OpenCode) has a dedicated adapter that configures hook scripts, normalizes lifecycle events into a common schema, and tracks session state (idle, thinking, tool calling, waiting for permission, error, ended).

The adapter system uses a shared HTTP server (`AgentHookRouter`) that listens on `127.0.0.1` and routes hook events and status updates to the correct session by URL path. Each session gets a unique auth token validated via `X-Canopy-Auth` header with timing-safe comparison. Hook commands are injected into each agent's configuration using `.sh` scripts on macOS/Linux. On Windows, Claude Code still uses `.sh` (its CLI runs hooks via its own bash layer), while Codex and Gemini CLI use `.cmd` wrappers — those CLIs execute hook commands through the system shell, and if `.sh` files are associated with Git Bash or WSL bash, each hook event would open a new visible terminal window.

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

**Claude Code:** Writes a temporary `settings.json` at `{userData}/canopy/agent-hooks/session-{uuid}.json` with hooks for 18 event types and an optional `statusLine` command. Passes `--settings {path}` to the CLI. Supports `--model`, `--permission-mode`, `--effort`, `--append-system-prompt` from preferences. Env vars: `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`, provider flags (`CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, `CLAUDE_CODE_USE_FOUNDRY`), and arbitrary custom env vars (with blocklist filtering).

**Codex:** Writes hooks to `.codex/hooks.json` inside the worktree directory. Adds `.codex/` to `.gitignore` if not already present. Uses refcounting for concurrent sessions sharing the same worktree. On cleanup, restores the original `hooks.json` content (or removes the file/directory if Canopy created it). Passes `--enable hooks` plus `--model`, `--ask-for-approval`, `--sandbox`, `--full-auto`, `--dangerously-bypass-approvals-and-sandbox`, `--profile` from preferences. Observes prompt, tool, compact, subagent-stop, and idle lifecycle hooks without returning hook decisions. Env vars: `OPENAI_API_KEY`, `OPENAI_BASE_URL`, custom env.

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
| `BeforeCompact`       | `PreCompact`         | `PreCompact`       | `PreCompress`                  | `SessionCompacting` |
| `AfterCompact`        | `PostCompact`        | `PostCompact`      | -                              | `SessionCompacted`  |
| `Notification`        | `Notification`       | -                  | `Notification`                 | `TodoUpdated`       |
| `AfterToolUseFailure` | `PostToolUseFailure` | -                  | -                              | -                   |
| `SubagentStart`       | `SubagentStart`      | -                  | -                              | -                   |
| `SubagentStop`        | `SubagentStop`       | `SubagentStop`     | -                              | -                   |
| `TaskCompleted`       | `TaskCompleted`      | -                  | -                              | -                   |
| `TeammateIdle`        | `TeammateIdle`       | -                  | -                              | -                   |

Events that do not map to a known name are normalized as `Unknown`. Some events are subscribed to
deliberately without a normalized name, because only their payload is wanted: Claude Code's
`PreModelSwitch`/`PostModelSwitch` (2.1.251+) and Gemini's `BeforeModel`/`AfterModel` all resolve to
`Unknown`. `handleHookEvent` assigns `session.model` from any event that carries `model` before it
branches on the event name, so a model switch updates the Agent Inspector without needing one.

### Session state tracking

The renderer maintains per-session state in `agentSessions[ptySessionId]`:

- `status`: Discriminated union with types `inactive`, `starting`, `idle`, `thinking`, `compacting`, `toolCalling`, `waitingPermission`, `error`, `ended`.
- `model` / `modelId`: Model name and ID (updated from status line data or hook events).
- `contextPercent` / `contextSize`: Context window usage. Claude provides this via the status line. Gemini calculates it from `AfterModel.usageMetadata.totalTokenCount` divided by a lookup table of model context limits (`resources/gemini-models.json`).
- `costUsd` / `durationMs` / `linesAdded` / `linesRemoved`: Cost tracking (Claude only via status line).
- `tasks`: Task list populated from `TaskCreate`/`TaskUpdate` tool calls (Claude, Codex) or `TodoUpdated` events (OpenCode). Capped at 50 tasks; oldest completed tasks are evicted first.
- `notifications`: Rolling buffer of 20 notification events.
- `activeSubagents`: Tracked via `SubagentStart`/`SubagentStop` events where available. Claude emits both start and stop; Codex currently contributes stop metadata when provided by its hook payload.
- `compactCount` / `toolCallCount`: Counters incremented on relevant events.
- `extra`: Agent-specific data (Claude rate limits, Codex `cwd`/`transcriptPath`/`turnId`, OpenCode pending questions).

Runtime session state is not reset when tab snapshots are reapplied. `initAgentSession()` is
idempotent for an existing PTY session and `paneFromSnapshot()` rekeys the renderer state when a
running agent pane receives a new `sessionId`, preserving status, badges, model/context data, tasks,
notifications, and counters across tab/layout updates.

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

Canopy stores two identifiers for each agent PTY. The hook route uses an internal `hookSessionId`
allocated before process spawn, while `agentSessionId` is updated from normalized hook payloads
when the CLI reports its real conversation/session ID. Resume commands always use
`agentSessionId`; falling back to the internal hook route ID would make Claude/Codex report
"session not found".

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

**Launching a profile.** The Tools sidebar renders each AI agent as a collapsible group when it has two or more profiles. Expanding the group lists the profiles; clicking one spawns the agent using that profile's configuration. When an agent has only a single profile (typically the `Default`), it renders as a flat launcher with no chevron — one click launches directly. If `profileId` is omitted from the tab command payload, the spawn handler falls back to reading global preferences (legacy behaviour).

**Profile -> adapter seam.** Adapters are profile-agnostic: they take a `PreferencesReader` interface (`{ get(key): string | null }`). When the tab command spawn path receives a `profileId`, it wraps the profile in a `profileToReader()` shim that returns the profile's values for `${agentType}.*` keys and delegates all other keys to the global `preferencesStore`. This means adding profile support required zero changes to `AgentSessionManager` or any of the four adapter files. All three reader call sites in the spawn path (settingsJson parsing, `getCliArgs`, `getEnvVars`) swap to the shim together.

**Storage.** Profiles live in the `agent_profiles` SQLite table with columns `id`, `agent_type`, `name`, `is_default`, `sort_index`, `prefs_json`, `api_key_enc`, `created_at`, `updated_at`. `api_key_enc` is encrypted with Electron's `safeStorage` (identical pattern to `CredentialStore`; falls back to plain base64 on Linux without a keyring). The name is unique per agent type. Only a single profile per agent type may be deleted down to — the store returns `ProfileLastDeletion` if the user tries to remove the last profile.

**Migration.** On first launch after the feature lands, `ProfileStore.ensureDefaults()` runs (inside `app.whenReady()`, after `safeStorage` is initialized) and — for each agent type with zero profiles — reads the legacy `${agentType}.*` keys from the `preferences` table and inserts a `Default` profile with those values. The legacy rows are left in place so downgrades remain safe. The migration is idempotent: it runs on every startup but is a no-op once profiles exist.

**API key masking.** Profiles cross IPC as `AgentProfileMasked`, which replaces the decrypted `apiKey` with a boolean `hasApiKey`. The renderer never sees the decrypted key. On save, the renderer sends `apiKey: undefined` to keep the existing key, `apiKey: null` to clear it, or a new string to overwrite. The main process reads the decrypted key only via `ProfileStore.getInternal()`, which is never exposed over IPC.

**Layout restore.** `PaneSession.profileId` is serialized with tab layouts so restored tabs re-spawn with the same profile. The tab display name appends the profile name when it is not `"Default"` (e.g. `Claude Code (Ollama)`).

## Configuration

Preferences for each agent are organized into **profiles** (see the Profiles section above). The per-agent configuration UI lives at Settings → Claude / Gemini / OpenCode / Codex and renders a two-pane list-and-form editor backed by `AgentProfilesPanel.svelte`. Existing global preferences from earlier versions are migrated into a `Default` profile automatically.

The fields below describe the keys stored inside each profile's `prefs_json` (non-secret) and the separately encrypted `api_key_enc` column. Adapters read them via `${agentType}.<field>` lookups on the `PreferencesReader` shim.

| Profile field                          | Agent    | Purpose                                                                                                        |
| -------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------- |
| `model`                                | Claude   | `--model` argument                                                                                             |
| `permissionMode`                       | Claude   | `--permission-mode` argument                                                                                   |
| `effortLevel`                          | Claude   | `--effort` argument                                                                                            |
| `appendSystemPrompt`                   | Claude   | `--append-system-prompt` argument                                                                              |
| `apiKey` _(encrypted)_                 | Claude   | `ANTHROPIC_API_KEY` env var                                                                                    |
| `baseUrl`                              | Claude   | `ANTHROPIC_BASE_URL` env var                                                                                   |
| `provider`                             | Claude   | Sets `CLAUDE_CODE_USE_BEDROCK`/`VERTEX`/`FOUNDRY`                                                              |
| `customEnv`                            | Claude   | JSON object of additional env vars                                                                             |
| `settingsJson`                         | Claude   | Merged into per-session `settings.json`                                                                        |
| `model`                                | Codex    | `--model` argument                                                                                             |
| `approvalMode`                         | Codex    | `--ask-for-approval` argument                                                                                  |
| `sandbox`                              | Codex    | `--sandbox` argument                                                                                           |
| `fullAuto`                             | Codex    | `--full-auto` flag (when `"true"`)                                                                             |
| `dangerouslyBypassApprovalsAndSandbox` | Codex    | `--dangerously-bypass-approvals-and-sandbox` flag; takes precedence over approval mode, sandbox, and full auto |
| `profile`                              | Codex    | `--profile` argument                                                                                           |
| `apiKey` _(encrypted)_                 | Codex    | `OPENAI_API_KEY` env var                                                                                       |
| `baseUrl`                              | Codex    | `OPENAI_BASE_URL` env var                                                                                      |
| `customEnv`                            | Codex    | JSON object of additional env vars                                                                             |
| `settingsJson`                         | Codex    | Merged into per-session `.codex/hooks.json`                                                                    |
| `model`                                | Gemini   | `--model` argument                                                                                             |
| `approvalMode`                         | Gemini   | `--approval-mode` argument                                                                                     |
| `apiKey` _(encrypted)_                 | Gemini   | `GEMINI_API_KEY` env var                                                                                       |
| `customEnv`                            | Gemini   | JSON object of additional env vars                                                                             |
| `settingsJson`                         | Gemini   | Merged into per-session `.gemini/settings.json`                                                                |
| `model`                                | OpenCode | `--model` argument                                                                                             |
| `apiKey` _(encrypted)_                 | OpenCode | `ANTHROPIC_API_KEY` env var                                                                                    |
| `settingsJson`                         | OpenCode | `OPENCODE_CONFIG_CONTENT` env var                                                                              |
| `customEnv`                            | OpenCode | JSON object of additional env vars                                                                             |

Custom env vars are filtered against a blocklist (`BLOCKED_ENV_VARS` from `security/envBlocklist`) and internal vars (`CANOPY_HOOK_PORT`, `CANOPY_HOOK_TOKEN`, `ELECTRON_RUN_AS_NODE`).

A few opt-in Claude Code variables are worth setting through `customEnv` when running many agent sessions at once (all pass the blocklist unchanged; none has a Canopy default):

- `CLAUDE_CODE_TOOL_MEMORY_LIMIT` — caps Bash tool commands with a memory cgroup on Linux (Claude Code 2.1.233+). Canopy hosts several agent PTYs per workspace, so a runaway build in one session competes with every other pane; this bounds it instead of letting it stall the session.
- `CLAUDE_CODE_WEBFETCH_CACHE_TTL_MS` — WebFetch session URL cache TTL, default 15 minutes (Claude Code 2.1.233+).
- `CLAUDE_CODE_SUBAGENT_MODEL` with `CLAUDE_CODE_SUBAGENT_MODEL_FORCE` (Claude Code 2.1.257+) — the second applies the first (or the main model) to _every_ subagent, ignoring per-spawn and agent-definition model overrides. Canopy already counts subagents per pane (`activeSubagents`, from `SubagentStart`/`SubagentStop`), and a workspace running several panes fans those out well past what one session would; forcing a cheap model on all of them is the one lever that a repository's own agent definitions cannot override. It is blunt for the same reason — a worktree that deliberately pins a strong model to one subagent loses that pin too.

One further variable passes the blocklist but should be left unset: `CLAUDE_CODE_PROJECT_DIR_NAME` (Claude Code 2.1.234+) names the per-project transcript directory inside the Claude config directory. It exists for hosts that give each session its own config directory — Canopy does not. Claude Code sessions share the user's `~/.claude`; only `--settings` is per-session (unlike Gemini and OpenCode, which do get isolated directories). A profile holds one fixed value, so setting it would point every worktree that runs the profile at the same transcript directory instead of one per worktree.

**Making `/model` picks survive a resume.** Canopy appends `--model` from the profile's Model field on every spawn _and_ every resume — `getResumeArgs` runs first, `getCliArgs` immediately after (`commands/tabCommands.ts`) — so an explicit flag re-asserts the profile's model each time the session restarts, overriding whatever `/model` the user picked inside the pane. To let a `/model` pick stick instead, leave the Model field blank and set `ANTHROPIC_DEFAULT_MODEL` in `customEnv` (Claude Code 2.1.236+): it sets the model new sessions start on, and a `/model` override persists across restarts. `ANTHROPIC_MODEL` is not a substitute — it forces the model and `/model` cannot override it.

**Curating the `/model` list with `modelPicker`.** Once the Model field is blank, `/model` inside the
pane is what chooses the model — and Claude Code 2.1.243+ accepts a `modelPicker` setting that
replaces or extends the built-in lineup with an ordered, labeled list. It reaches the CLI through the
profile's Settings JSON field like any other override. This matters more on Canopy than on a bare
terminal because the Model field's own hint suggests short names (`sonnet`, `opus`, `haiku`, `fable`)
while Bedrock, Vertex and Foundry profiles need those providers' own id spellings — `modelPicker`
accepts any spelling, so a provider profile can offer the ids that actually work on it instead of
leaving the user to type one.

A gateway profile can now skip the curation entirely. 2.1.257 lets a gateway supply a description
alongside each entry it advertises under `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY`; entries
without one still read "From gateway". The Base URL field exists for exactly these profiles — its own
hint names Ollama, GLM and MinMax — so a gateway that describes its models makes the picker
self-documenting and leaves `modelPicker` for the provider profiles that have no discovery endpoint
to ask.

**`keybindingFlavor: "readline"` only takes effect on macOS.** Claude Code 2.1.238+ accepts a
`keybindingFlavor` setting; `"readline"` makes Ctrl+W in its prompt delete back to the previous
whitespace, as in Bash (the default `"classic"` is unchanged). It reaches the CLI through the
profile's Settings JSON field, which is merged into the per-session `settings.json`. On Windows and
Linux the modifier for Canopy's own shortcuts is Ctrl, so Ctrl+W matches the global "close pane (or
tab if last)" binding in `MainLayout.svelte` and the pane closes instead of a word being deleted —
Canopy's shortcuts are fixed, so there is no remap to work around it. On macOS the modifier is ⌘,
Canopy ignores Ctrl+W, and the readline binding works as documented.

**Bedrock, Vertex and Foundry profiles now get the fullscreen renderer.** Claude Code 2.1.239
extends its one-time fullscreen renderer offer to those providers, which were previously excluded,
and fresh installs on them start in fullscreen. Canopy sets `CLAUDE_CODE_USE_BEDROCK`,
`CLAUDE_CODE_USE_VERTEX` and `CLAUDE_CODE_USE_FOUNDRY` from the profile's Provider field, so any
pane running such a profile is affected. Fullscreen is the alt-screen renderer with its own
virtualized scrollback, so the transcript stops accumulating in the pane's xterm scrollback
(`scrollback: 5000` in `TerminalInstance.svelte`) and `scrollPreservingWrite` has nothing to
preserve — scrolling that session means scrolling inside Claude Code, not the pane. To keep the
previous main-screen rendering, put `{ "tui": "default" }` in the profile's Settings JSON field; it
is merged into the per-session `settings.json` like any other override.

Claude Code 2.1.246 fixes the two fullscreen defects that hit Canopy hardest: a blank transcript
after the terminal is resized, which recovered only on the next keypress, and erratic scrolling when
the view sat at an earlier message, including jump-to-bottom sticking mid-transcript. Both are
easier to reach in a Canopy pane than in a standalone terminal, because Canopy drives the resize
path from three places a plain terminal does not — a debounced `ResizeObserver` on each pane
container, a `resizePty` re-assert on focus and click, and another on reattach, all in
`TerminalInstance.svelte` — so splitting a pane, resizing the window and switching tabs each trigger
it. If a Bedrock, Vertex or Foundry profile was switched to `{ "tui": "default" }` to escape blank or
jumpy output rather than out of a preference for main-screen rendering, that reason no longer
applies once the user's `claude` binary is on 2.1.246 or later.

**`promptCacheTtl` is worth raising for panes you leave parked.** Claude Code 2.1.243+ accepts
`promptCacheTtl` and `subagentPromptCacheTtl`, which extend the prompt cache from the default 5
minutes to 1 hour. Both reach the CLI through the profile's Settings JSON field. They were added for
API-key and cloud-provider users, which is every Canopy pane — Canopy authenticates Claude Code with
`ANTHROPIC_API_KEY`/`ANTHROPIC_BASE_URL` or the Bedrock/Vertex/Foundry flags, never an interactive
subscription login. The usage pattern fits too: agent panes sit parked across worktrees and tabs and
get returned to well after five minutes, so under the default TTL the cache has expired and each
return re-writes the whole prefix.

It is a cost tradeoff, not a free win. Cache reads bill at roughly 0.1x base input either way, but a
cache _write_ costs 1.25x at the 5-minute TTL and 2x at 1 hour — so break-even moves from two reads
to three. Raising it pays off on panes returned to repeatedly with a large accumulated context, and
costs more on panes used once and closed. Raise `promptCacheTtl` alone and leave
`subagentPromptCacheTtl` at the default: that split is what the setting pair is for, since subagents
are short-lived and usually never re-read their prefix at all.

**`feedbackDrafts` is worth turning off on profiles that run against private repositories.** Claude
Code 2.1.247 adds a `SendFeedback` tool: when something goes wrong in a session, Claude can draft a
feedback report for the user to review and send from `/feedback`. The `feedbackDrafts` setting
disables it, and reaches the CLI through the profile's Settings JSON field like any other override.
Nothing leaves the machine until the user opens `/feedback` and sends it, so this is about what gets
drafted, not about silent egress — but what gets drafted in a Canopy pane is drawn from a session
working in a real worktree, so it can quote repository content, file paths and command output from
whatever the agent was doing when it failed. Because the setting is per-profile, a profile used for
client or private work can carry `{ "feedbackDrafts": false }` while a general-purpose profile leaves
drafting on.

Canopy surfaces the tool no differently from any other. `SendFeedback` arrives as an ordinary
`PreToolUse` event, so the notch shows `toolCalling` with the detail built by `summarizeToolInput`
like every other tool; `formatNotification` fires only on `PermissionRequest`, so there is no
separate notification. The draft itself stays inside the pane.

**`CLAUDE_CODE_RESTRICTED` passes the blocklist but should not be set on a Canopy profile.** Claude
Code 2.1.248 adds `--restricted` and its `CLAUDE_CODE_RESTRICTED=1` equivalent, which removes the
built-in tools that run commands or code plus WebFetch, keeps file tools inside the working
directory, refuses `bypassPermissions`, and ignores user, project and local settings files. Nothing
in it escalates privilege — it only subtracts capability — so `BLOCKED_ENV_VARS` correctly leaves it
alone, but three of those clauses land badly here. Removing the command and code tools removes most
of what an agent pane in a terminal workstation is for. Refusing `bypassPermissions` collides with
the profile's Permission Mode field, which feeds `--permission-mode` and accepts exactly that value,
so a profile setting both will not start. And the settings clause names user, project and local
files specifically; Canopy's hooks and status line ride the explicit `--settings {path}` flag, a
separate channel that should survive, but that is read off the release note rather than tested here.
If it did not survive, the notch status, tab badges, permission notifications and the
`agentSessionId` that `--resume` depends on would go dark together, because every one of them is fed
by the hook script written into that file.

**Bedrock, Vertex and Foundry panes can now message each other.** `SendMessage` and `ListAgents`
were previously unavailable on those providers and when telemetry is disabled; 2.1.248 enables both
for sessions on the same machine. Canopy is that case by construction — a workspace runs several
agent PTYs at once, each its own session — so a machine running these profiles now lists sibling
panes across the workspace's worktrees and tabs, not just agents spawned inside one pane. The
adapter needs no change: the resulting traffic arrives as the `SubagentStart`, `SubagentStop` and
`TeammateIdle` events `EVENT_MAP` already normalizes.

**Two prompt-cache fixes in 2.1.248, only one of which reaches Canopy.** The headline one — a cache
miss roughly once an hour, caused by tool definitions being re-rendered after an OAuth token refresh
— does not apply to Canopy panes, which authenticate with `ANTHROPIC_API_KEY`/`ANTHROPIC_BASE_URL`
or the Bedrock/Vertex/Foundry flags and never hold an OAuth token to refresh. The second one does:
on an account that had entered usage overage, the `ScheduleWakeup` tool definition differed between
a session and its `--resume`, costing a full cache miss on the resumed session's first turn.
`getResumeArgs` issues `--resume {agentSessionId}` on every layout restore, so that miss was paid
once per restored pane. It also changes the arithmetic in the `promptCacheTtl` note above: paying 2x
to write a one-hour cache only earns out if the session that returns can read it, and a restored
pane on an overage account could not.

**`experimental.cacheTtl` gives one agent definition its own TTL.** 2.1.248 accepts `cacheTtl`
(`"5m"` or `"1h"`) in agent frontmatter, used when no subagent TTL setting is configured. That is the
escape hatch for the split recommended above: a worktree can give a single long-lived subagent a
one-hour cache while `subagentPromptCacheTtl` stays unset profile-wide, instead of the all-or-nothing
choice the settings pair forces. It lives in the agent file in the repository rather than in the
profile, so it is scoped per worktree, not per profile.

**Canopy observes model switches but does not police them.** Claude Code 2.1.251 adds
`PreModelSwitch` and `PostModelSwitch` hooks, and `PreModelSwitch` can block or require confirmation
for a switch. Canopy subscribes to both, but only to read the model out of the payload — the hook
script always exits 0 and the hook server answers `{}` for every event except `SessionStart`, so no
Canopy pane can refuse a switch. That is deliberate. A profile's Model field already re-asserts
`--model` on every spawn and resume (see the `ANTHROPIC_DEFAULT_MODEL` note above), which is the
supported way to pin a model; using the hook to reject `/model` outright would make the pane fight
the user mid-session instead. A user who does want a hard block has to put the hook in their own
`~/.claude/settings.json`, which Claude Code merges as a separate source; the profile's Settings JSON
field will not work for it, because `setupSettings` spreads the profile's overrides and then writes
its own `hooks` key last, replacing any `hooks` the profile supplied. That applies to every hook
event, not just this one.

**Resumed sessions now report what the resume will cost.** 2.1.251 extends the `SessionStart` payload
on a resume with the session's staleness and an estimated re-cache cost. Canopy already answers
`SessionStart` — that is where `buildSessionContext` injects the workspace and worktree — and it
resumes on every layout restore, so this fires on each restored pane. `normalizeEvent` does not read
the new fields yet and nothing displays them. They are the missing input to the `promptCacheTtl`
tradeoff above: the decision to pay a 2x cache write turns on whether a parked pane will still be
warm when it is returned to, which is exactly what staleness reports.

**Two new status-line fields, neither displayed yet.** 2.1.251 adds `rate_limits.spend_limit`, for
accounts behind a Claude apps gateway with spend limits, and a top-level `prompt_cache` object (hit
ratio, misses, tokens re-cached, warm/cold). They reach Canopy differently. `normalizeStatus` passes
`rate_limits` through whole, so `spend_limit` arrives in the renderer intact, but
`handleStatusUpdate` flattens only `five_hour` and `seven_day` into the keys `ClaudeExtras` reads, so
it is carried and ignored. `prompt_cache` is dropped earlier: `normalizeStatus` reads five named keys
(`version`, `model`, `context_window`, `cost`, `rate_limits`) and discards the rest. Wiring either
into the Agent Inspector needs the field shapes confirmed against a real 2.1.251 status line first —
the release notes name the quantities but not the JSON keys.

**A fresh worktree had nowhere to keep "always allow", and every Canopy pane starts in one.** Claude
Code 2.1.252 fixes project-level "always allow" not saving in a project that has no
`.claude/settings.local.json` yet. That is the normal state of a Canopy pane rather than an edge
case: `worktree:create` hands `git worktree add` a path under the configured base directory
(`~/canopy/worktrees` unless `worktrees.baseDir` says otherwise), and that file is conventionally
gitignored, so a newly created worktree does not contain one. The per-session file Canopy does write
is not a substitute — `setupSettings` writes the hooks and `statusLine` to
`{userData}/canopy/agent-hooks/session-{uuid}.json`, passes it as `--settings {path}`, and
`unlinkSync`s it on cleanup, so it is neither the file Claude Code persists permission decisions to
nor one that would outlive the session if it were. Before the fix the symptom was Canopy-shaped
rather than silent: `formatNotification` raises an OS notification on every `PermissionRequest`, so a
choice that failed to persist came back as the same tool re-prompting and re-notifying in each new
worktree. The fix is entirely CLI-side and needs no adapter change.

**The macOS tasks-directory fix should not reach worktrees Canopy created.** 2.1.252 fixes Bash
commands failing with "task output swap refused (tasks dir moved or linked)" on some Macs — the
symlinked-path case that `/tmp` → `/private/tmp` and `/var` → `/private/var` produce there.
`validateWorktreeCreationPath` already returns `resolvedTarget` from `resolveWithExistingAncestor`,
which is realpath-canonicalized, and `worktree:create` passes exactly that value to
`GitRepository.worktreeAdd`, so a pane's working directory is the canonical path rather than an alias
of it. That canonicalization exists for the containment checks, not for this, but it removes the
precondition as a side effect. It does not cover a directory the user attached directly, which does
not go through that path.

**Canopy's 1 MB hook cap still drops what the CLI now truncates.** The fourth fix stops background
task notifications carrying very large failure output — the release note's example is a git error on
a full disk — from pushing the conversation past the API request size limit. `TaskCompleted` is in
`CLAUDE_HOOK_EVENTS` and `EVENT_MAP`, so that class of payload also reaches Canopy, where
`AgentHookServer`'s `MAX_BODY_BYTES` (1 MB) drops an oversized body silently and the renderer's task
list never sees the completion. Whether the upstream truncation also applies to what the hook
receives is not stated in the release note and is not tested here; if it does not, the existing
silent-drop row in the error table below is the behaviour to expect.

**Claude Code's "Remote Control" is not Canopy's.** 2.1.252 fixes Remote Control sessions hosted by
Claude Desktop or VS Code stalling for minutes after a tool finished when the connection to claude.ai
was degraded. That is a claude.ai-hosted session driven from those two editors, and Canopy is not a
host for it. Canopy's own Remote Control (`src/main/remote/`) is an unrelated WebRTC feature that
mirrors a Canopy window to a phone over the LAN; the names collide and nothing in this fix touches
it.

**Fable 5.1 breaks the 0.1x cache-read ratio the `promptCacheTtl` note assumes.** Claude Code 2.1.257
makes Claude Fable 5.1 (`claude-fable-5-1`) the default Fable model, with a 1M context and billing of
$10/$50 per Mtok against $0.25/Mtok for cache reads. That last figure is the one that matters here:
0.25/10 is **0.025x** base input, not the ~0.1x the rest of the lineup holds to and that the
`promptCacheTtl` arithmetic above is written around. A cached read on a Fable pane costs a quarter of
what the note assumes relative to input, so every case for raising `promptCacheTtl` gets
stronger and the cases against it — a pane used once and closed — are the only ones left. The 1M
context pushes the same way, because the prefix being written or re-written is that much larger.
Nothing needs changing in Canopy to display it: `contextWindow` is read straight off the status line
(`context_window_size` and `used_percentage`), so a 1M-context pane reports its own size.

**Auto mode is no longer full autonomy, and a Canopy pane meets the new prompt in every worktree.**
2.1.257 adds two holdouts to the Auto permission mode the profile's Permission Mode field selects. The
first is a Containment Escape rule: cloud metadata-credential fetches, egress evasion and cross-tenant
reach stop being auto-approved unless the environment marks them expected. The second is a one-time
prompt before the first file read outside the working directories, with the option to block such reads
outright. Canopy gives a session exactly one working directory — the worktree — and passes no
`--add-dir`, so the main checkout, every sibling worktree and anything under `~` is outside it. That
makes this the same shape as the 2.1.252 "always allow" fix above: `worktree:create` hands each task a
fresh directory, so "first read outside the working directory" is reached again in each new worktree
rather than once per machine. A profile that should never read outside can settle it in advance with
`{ "permissions": { "blockReadsOutsideWorkingDirectories": true } }` in the Settings JSON field.
Whether the prompt arrives as a `PermissionRequest` hook — and so raises the OS notification
`formatNotification` builds — is not stated in the release note and is not tested here.

**`.claude/` created mid-session now takes effect, which completes the 2.1.252 story.** 2.1.257 fixes
settings in a `.claude/` folder created after startup not being picked up until restart. Read against
the 2.1.252 note above, the two are one sequence: 2.1.252 made a project-level "always allow" actually
save into a worktree that had no `.claude/settings.local.json`, and until 2.1.257 the file it saved
was then ignored for the rest of that session. A Canopy pane is where the pair matters, because
`git worktree add` produces a directory with no `.claude/` in it and the pane's session is the one
that creates it. Neither fix touches the per-session `--settings` file Canopy writes, which is passed
by path and read at startup.

**Ctrl+T is Canopy's, and 2.1.257 is the first release where that is fixable.** Claude Code gains an
`Agents` keybindings context: `keybindings.json` rebinds of Ctrl+G are no longer ignored in claude
agents, and its Ctrl+S / Ctrl+T are now rebindable there. Of those three, only Ctrl+T collides in a
Canopy pane. `handleKeydown` is bound with `<svelte:window onkeydown={...}>` in `MainLayout.svelte`
and has no exemption for a focused terminal, so on Windows and Linux — where the modifier is Ctrl —
Ctrl+T opens a new tab whichever pane has focus. It is a bubble-phase listener rather than a capture
one, so whether the keystroke _also_ reaches Claude Code through xterm depends on ordering that is not
tested here; the tab opens either way. Ctrl+S and Ctrl+G pass through untouched — Canopy's only
`Mod-s` binding lives inside `CodeMirrorEditor.svelte`, a different pane type, the Electron menu binds
only `CmdOrCtrl+,` and `CmdOrCtrl+Shift+N`, and `attachCustomKeyEventHandler` in
`TerminalInstance.svelte` intercepts only Ctrl+V, Ctrl+C, Cmd+Backspace and Ctrl+Z. Unlike
the Ctrl+W case above, rebinding is a real fix here rather than a workaround that defeats the point:
Ctrl+W is wanted _because_ it is Ctrl+W, whereas the agents-context actions just need some reachable
key. Users pick that key in their own `~/.claude/keybindings.json`, which Canopy shares — the
profile's Settings JSON field is the wrong channel for it.

**A closed pane can leave a sandbox mask file behind.** 2.1.257 adds a `/doctor` warning for stale
sandbox mask files left by a killed session. Canopy kills sessions as a matter of course — closing a
pane, closing a tab, quitting the app — so these accumulate faster here than under a terminal the user
exits cleanly. There is nothing to change in the adapter; it is worth knowing that `/doctor` is where
the residue shows up, and that a user reporting it has usually just been closing panes.

**Also in 2.1.257, with no Canopy consequence.** New `timeFormat` and `timeZone` settings (12-hour,
24-hour, 24-hour UTC, or a strftime pattern) control the turn-end clock and transcript timestamps;
they reach the CLI through the profile's Settings JSON field like any other override, and a pane
otherwise inherits the app process's timezone. `/effort` gains an `s` option for changing effort for
the current session only — the same relationship the Model field has with `/model`, since the profile's
Effort level field re-asserts `--effort` on every spawn and resume. Two background-session start
failures are fixed, on macOS npm installs mid self-update and on Windows behind a stale daemon lock
file pointing at a reused process id; those are Claude Code's own background sessions, not Canopy
panes. Finally, the release moves a large amount of prompt text: total prompt tokens are up 31.2%
(+4,253), with the system share going from 30.7% to 47.2% of the mix. That comes off the usable
context of every pane, which is visible in the Agent Inspector's context percentage but needs no code
change.

**The macOS 12 fix in 2.1.258 reaches Canopy through one narrow path, and the SDK pin is what
selects it.** 2.1.258 fixes Claude Code failing to launch on macOS 12 (Monterey), a regression from
2.1.255. Canopy runs two different Claude Code installations and only one of them is ours. A pane
spawns the `claude` binary the user installed, so a pane on a broken release is fixed by the user
updating, not by anything in this repo. The exception is `commitMessageGenerator.ts`, the one place
that imports `@anthropic-ai/claude-agent-sdk`: it resolves the user's binary with `which claude` and
passes it as `pathToClaudeCodeExecutable`, but that argument is `undefined` when the lookup fails,
and the SDK then falls back to the CLI it vendors itself. That vendored binary is pinned by
`package.json`. The mapping is exact rather than inferred — the installed package's `manifest.json`
carries `"version": "2.1.207"` alongside a per-platform binary table for SDK `0.3.207`, so `0.3.N`
vendors CLI `2.1.N`, and the `0.3.257` pin that preceded this release vendored the broken build. The
affected user is therefore on macOS 12 with no `claude` on `PATH`, and the symptom is commit message
generation failing — a shape that reads as a Canopy bug rather than a CLI launch failure, since that
user has no pane to see the same error in. Whether Canopy still runs on macOS 12 at all is a separate
question not settled here: `electron-builder.yml` sets no `minimumSystemVersion`, so the floor is
whatever Electron 43 carries.

**2.1.255 is referenced by Anthropic despite being recorded here as never released.** The note above
attributes the regression to 2.1.255, and the analysis of the v2.1.224 → v2.1.257 range concluded
that v2.1.253–v2.1.256 were never released, on the evidence that npm jumps `0.3.252` → `0.3.257` and
none of the four has a CHANGELOG entry. Both cannot be read literally. The reconciliation that fits
the evidence is that the npm SDK line is a republication of the CLI rather than the CLI itself, so a
version can exist upstream without ever being pushed to npm under `0.3.N`. That weakens npm gaps as
proof a CLI version does not exist — they are good evidence about what the _SDK_ shipped, which is
what our pin controls, and weaker evidence about what Anthropic built. Treat the earlier
"never released" conclusions as "never released to npm" until something confirms otherwise.

**The re-sent-approval fix does not reach Canopy, for two independent reasons.** 2.1.258 fixes remote
and scheduled sessions failing with "user messages must have non-empty content" after a re-sent
permission approval could not be applied. First, "remote" here is the same claude.ai-hosted session
covered in the 2.1.252 note above, not Canopy's WebRTC Remote Control — the names still collide and
this fix still does not touch it. Second, and more decisive, Canopy never sends a permission approval
in the first place: the adapter treats `PermissionRequest` as read-only, mapping it to
`waitingPermission` for the notch and to an OS notification through `formatNotification`. The user
answers in the pane. Nothing under `src/renderer/src/remote/` handles permissions at all.

**The whole of 2.1.258's prompt growth is system text; tool descriptions did not move.** Prompt
tokens are up 25.9% (+4,642) with one new prompt file (12 → 13), and the mix goes from 47.2%/52.8%
system/tools to 58.1%/41.9%. The falling tools share is dilution, not a reduction: the percentages
put the total at ~17.9k before and ~22.6k after, which holds tools flat at ~9.45k while system rises
~8.5k → ~13.1k. That is the second consecutive release with this exact shape — 2.1.257 added +4,253
against the same flat ~9.45k of tools — so across the two, system prompt has roughly tripled from
~4.2k to ~13.1k while every tool description stayed put. For Canopy the two halves land differently.
Flat tool descriptions mean nothing is required of the code that keys off tool names and shapes
(`summarizeToolInput`, the tool views, the `PreToolUse`/`PostToolUse` normalization), which is where
a tools-side change would have forced work. The system growth is pure overhead: ~9k tokens off the
usable context of every pane in two releases, visible as a higher starting context percentage in the
Agent Inspector and as compaction arriving sooner. What the new file contains is not recoverable from
the release notes, and the diff was not reachable this run — see the blocker note in
`.github/prompts/claude-code-compat.md`.

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
- Codex hook integration is observational except for `SessionStart` context injection; Canopy does not return Codex hook decisions, permission decisions, or tool-input rewrites.
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
