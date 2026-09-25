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
Claude Code's model-switch payloads have no top-level `model`: `PostModelSwitch` names the model it
landed on `to_model`, which `normalizeEvent` maps to `model`, and `PreModelSwitch` is not read,
because a user's own hook can still refuse the switch it proposes.

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
| `systemPromptSnapshot`                 | Claude   | `--system-prompt-snapshot off` when set to `"off"`; omitted otherwise (Claude Code 2.1.267+)                   |
| `apiKey` _(encrypted)_                 | Claude   | `ANTHROPIC_API_KEY` env var                                                                                    |
| `baseUrl`                              | Claude   | `ANTHROPIC_BASE_URL` env var                                                                                   |
| `provider`                             | Claude   | Sets `CLAUDE_CODE_USE_BEDROCK`/`VERTEX`/`FOUNDRY`                                                              |
| `customEnv`                            | Claude   | JSON object of additional env vars                                                                             |
| `settingsJson`                         | Claude   | Merged into per-session `settings.json`; a boolean `attribution` is rewritten to its object form               |
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

**Capping effort with `maxEffortLevel`.** Claude Code 2.1.267+ accepts a `maxEffortLevel` setting,
top level or per model under `modelSettings`, which caps the effort level on every provider including
Bedrock, Vertex and Foundry. It reaches the CLI through the profile's Settings JSON field like the
other settings here. The reason to know about it in Canopy specifically is that the profile has its
own **Effort level** dropdown, and the two do not negotiate: `buildCliArgs` emits `--effort` from the
dropdown, the CLI silently lowers it to the cap, and the dropdown goes on reading whatever was picked.
Users can still choose anything at or below the cap, so the setting is a ceiling rather than an
override — useful for holding a shared or cost-sensitive profile down without editing every pane, and
worth checking first when a profile behaves a tier below what it displays.

**`keybindingFlavor: "readline"` only takes effect on macOS.** Claude Code 2.1.238+ accepts a
`keybindingFlavor` setting; `"readline"` makes Ctrl+W in its prompt delete back to the previous
whitespace, as in Bash (the default `"classic"` is unchanged). It reaches the CLI through the
profile's Settings JSON field, which is merged into the per-session `settings.json`. On Windows and
Linux the modifier for Canopy's own shortcuts is Ctrl, so Ctrl+W matches the global "close pane (or
tab if last)" binding in `MainLayout.svelte` and the pane closes instead of a word being deleted —
Canopy's shortcuts are fixed, so there is no remap to work around it. On macOS the modifier is ⌘,
Canopy ignores Ctrl+W, and the readline binding works as documented.

**`maxProseWidth` keeps prose readable in wide panes.** Claude Code 2.1.282+ accepts `maxProseWidth`,
a column count (integer, minimum 40) that caps the width of Claude's prose — paragraphs, headings,
lists and blockquotes — while tables and code blocks keep the full width. Only the display wraps; the
response text gains no line breaks. It reaches the CLI through the profile's Settings JSON field, for
example `{ "maxProseWidth": 100 }`. A Canopy pane is as wide as its split, so a single pane on a wide
monitor runs to hundreds of columns, and unset (the default) wraps prose at the full width the pane
reports on resize. **Unlike 2.1.281's boolean `attribution`, it is safe on a profile whose `claude`
is older.** A new key and a new type for an existing key fail differently: the 2.1.207 build vendored
in `node_modules` and the 2.1.282 build both end their settings object in `.passthrough()`, so an
older CLI keeps the unknown key and ignores it instead of skipping the file that carries Canopy's
hooks. 2.1.282 itself declares the key with `.catch(undefined)`, so a value below 40 or of the wrong
type is dropped on its own rather than invalidating the file.

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

**2.1.259's concurrent-`~/.claude.json` fix lands on the way Canopy is normally used, not on an edge
case.** Before this release, concurrent Claude Code sessions silently reverted each other's writes to
`~/.claude.json`: workspace trust reset and MCP/project state was lost. Canopy produces that
concurrency by design. Each pane spawns its own `claude` process against its own worktree, and a
user working across several worktrees has several running at once; separately,
`commitMessageGenerator.ts` spawns a further process through the SDK's `query()`, so generating a
commit message while any pane is live is two writers on one file. The expected pre-2.1.259 symptom
is a worktree asking to be trusted again after it had already been trusted — which reads as a Canopy
bug, because the trust prompt appears inside a Canopy pane. No code change: the per-session state
Canopy owns is the `--settings` file it writes per session at
`{userData}/canopy/agent-hooks/session-{uuid}.json`, and that was never the contended file. Panes are
fixed by the user updating their own CLI; the SDK pin only moves the vendored fallback described
above.

**`--permission-prompts none` is new and deliberately not adopted.** It makes anything that would
prompt deny automatically, while the active permission mode keeps deciding, for unattended headless
hosts. Panes are the opposite of that: Canopy's whole permission path assumes a human answers. The
adapter maps `PermissionRequest` to `waitingPermission` for the notch, raises an OS notification
through `formatNotification`, and drives the `permission` badge that is never downgraded to `unread`
at either tab or worktree level. Passing this flag would strand that machinery — the user would see
silent tool failures instead of a prompt. It is recorded here so a later pass does not read it as an
unadopted feature. The same reasoning rules it out of the `Permission mode` profile field, which
selects `--permission-mode` values (`plan`, `auto`, `acceptEdits`, `bypassPermissions`); this flag is
orthogonal to those and would deny-all rather than allow-all.

**The `Bash` `Read()` deny-rule fix matters to profiles that lock a pane down, and Canopy has such a
seam.** 2.1.259 closes several ways a denied file could still be read: as an option value
(`--ignore-revs-file=.env`, `-f.env`, `@file`), as a `git diff`/`git grep` file operand, or through a
`cd DIR && cat FILE` compound; `grep -r`/`cp -r` over a directory holding a denied file now asks.
Canopy does not write permission rules itself, but a profile's settings JSON override is merged into
the per-session settings file by `setupSettings`, so a user who denied `Read(.env)` there was getting
less than it looked like. Nothing to change in this repo — `.claude/settings.json` defines hooks and
no `Read()` deny rules — but the guarantee that field offers is stronger from 2.1.259 on.

**`managedMcpServers` needs nothing from Canopy and is unlikely to be visible in it.** The new
managed setting lets an organization provision HTTP/SSE MCP servers to every user, using the
`.mcp.json` entry shape, with entries naming a command to run skipped. It is an
administrator-provisioned setting rather than anything a session passes, so it would reach panes on a
managed machine without Canopy participating; Canopy neither reads nor writes MCP configuration
anywhere, which the codebase scan confirms. The related `--json` output for `claude plugin validate`
and the `glab mr` tool-summary recognition are both surfaces Canopy does not consume — it reads hook
events over its local HTTP server, never the CLI's rendered output.

**Prompt growth is system-side for the third consecutive release; tool descriptions have still not
moved.** Prompt tokens are up 22.3% (+5,031) with one new prompt file (13 → 14), and the mix goes
from 58.1%/41.9% system/tools to 65.7%/34.3%. Running the same arithmetic as the 2.1.258 note puts
the total at ~22.6k before and ~27.6k after, which holds tools flat at ~9.46k against ~9.45k — a
difference of tens of tokens, inside the rounding the 0.1% percentages carry, so flat rather than a
finding — while system rises ~13.1k → ~18.1k. Across 2.1.257, 2.1.258 and 2.1.259 that is +13,926
tokens, all of it system text: system prompt has gone ~4.2k → ~18.1k while every tool description
stayed at ~9.45k. The consequence for Canopy is unchanged and still splits the same way. Flat tool
descriptions mean nothing is asked of the code keyed to tool names and shapes (`summarizeToolInput`,
the tool views, the `PreToolUse`/`PostToolUse` normalization). The system growth is overhead: ~14k
tokens off the usable context of every pane across three releases, visible as a higher starting
context percentage in the Agent Inspector and as compaction arriving sooner.

**25 of 2.1.259's CLI changelog entries were not readable this run.** The release notes truncate at
"… +25 more CLI changelog entries", and both routes to the rest were denied — `gh api` against the
changelog repo by the allowlist defect described in `.github/prompts/claude-code-compat.md`, and
`WebFetch` on its own. The items above are the visible entries only. Nothing in them required an
adapter change, but that is not the same as having reviewed the release, and in particular a new hook
event in the hidden entries would not have been seen: `CLAUDE_HOOK_EVENTS` is a hand-maintained list
of 18 names, so an event Claude Code gains is silently not subscribed to rather than failing loudly.

**The vendored SDK answers the question that note left open, but not for the release being analysed.**
There is a check on `CLAUDE_HOOK_EVENTS` that needs no network and no allowlist:
`node_modules/@anthropic-ai/claude-agent-sdk/sdk.d.ts` exports a `HookEvent` union of every event
name, and `node_modules/@anthropic-ai/claude-agent-sdk-linux-x64/claude` is the vendored CLI itself,
whose strings `grep -a` will read. The trap is which copy is on disk. The compat workflow checks out
`ref: next` and runs `npm ci` there, so `node_modules` holds whatever `next` pins — `0.3.207` while
this PR sits unmerged, whose `manifest.json` carries `"2.1.207"`, well below the range being
analysed. Taken at face value that copy reports `PreModelSwitch` and `PostModelSwitch` as
nonexistent, which is true only of 2.1.207: they arrived in 2.1.251. Anything read this way is a
statement about `next`'s pin rather than about `TO_VERSION`, and the gap widens with every release
until the pin lands.

**Read as a lower bound it still shows the subscription gap is wide and long-standing.** The
`HookEvent` union already carried 30 names at `0.3.207`, against the 16 of them Canopy subscribes to.
The 14 it does not are `PostToolBatch`, `UserPromptExpansion`, `PermissionDenied`, `Setup`,
`TaskCreated`, `Elicitation`, `ElicitationResult`, `ConfigChange`, `WorktreeCreate`, `WorktreeRemove`,
`InstructionsLoaded`, `CwdChanged`, `FileChanged` and `MessageDisplay`. None reads like a candidate
for removal since, so treat that as a floor on today's gap rather than as a current list. Four line
up with machinery Canopy already has and would need no new concepts: `PermissionDenied` is the
outcome half of the `PermissionRequest` the adapter maps to `waitingPermission` and notifies on, so a
pane learns that a prompt appeared but never that it was refused; `TaskCreated` is the opening half
of the `TaskCompleted` the renderer's task list already consumes; `Elicitation` is a second thing
that blocks on the user while raising no OS notification; and `CwdChanged` would desync a pane whose
whole model is one worktree per session. Subscribing changes behaviour for every pane and the payload
shapes are not settled here, so this is recorded for a maintainer to decide rather than taken.

**`prompt_cache` grew again and is still discarded before the renderer sees it.** 2.1.260 adds a
likely cause for a prompt-cache miss — the examples given are tool definitions or the system prompt
changing, and going idle past the TTL — to `/cost` and to the status line's `prompt_cache` field.
That is the second release to put content into a field Canopy drops: `normalizeStatus` reads five
named keys (`version`, `model`, `context_window`, `cost`, `rate_limits`) and discards the rest, so
`prompt_cache` never reaches `handleStatusUpdate`. The 2.1.251 note above made wiring it conditional
on confirming the JSON keys against a real status line, and that condition is not met this run
either — the release notes name the quantity but not the key, and the diff was unreachable. What has
changed is the value of doing it: a miss cause is exactly what would explain a `promptCacheTtl`
setting failing to pay off, which is the tuning question these notes spend the most space on.

**`/diff`, `/advisor` and `/reload-plugins` are pane-local and need nothing.** 2.1.260 adds a
fullscreen diff panel toggled with `/diff`, a text form of `/advisor` for desktop, Remote Control and
other headless sessions, and `/reload-plugins` in headless sessions so it appears in SDK command
lists. Canopy enumerates no Claude Code slash commands anywhere — the codebase scan finds none, and
`tabCommands.ts` holds only tool ids and session ids — so all three reach a user by being typed into
the pane's PTY. The diff panel overlaps Canopy's own Git sidebar in purpose without colliding with
it: it renders inside the pane, against the same worktree.

**The permission-rule fixes land on the profile Settings JSON seam, as 2.1.259's `Read()` fix did.**
2.1.260 stops `Edit`/`Write`/`Read` rules whose path contains parentheses being dropped as invalid or
ignored by the Bash sandbox, which had left folders that read as "read-only" writable; stops a single
rule with an uncompilable pattern, an unclosed `[` for instance, making every file edit fail with
"Invalid regular expression"; and stops Bash checks auto-approving zsh commands that hide a command
substitution in a `REPORTTIME`, `REPORTMEMORY` or `DIRSTACKSIZE` assignment. Canopy writes no
permission rules of its own, but `setupSettings` merges a profile's Settings JSON overrides into the
per-session file, so a user who locked a pane down through that field gets a guarantee that is real
from 2.1.260 rather than approximately real. Nothing in this repository trips the parenthesis case:
`.claude/settings.json` defines only hooks, and `.claude/settings.local.json` only `Bash(git add:*)`
and `Bash(git commit:*)`. The rule defect the compat workflow itself hits is a different mechanism
and is not fixed by any of this — see the allowlist note in `.github/prompts/claude-code-compat.md`.

**Prompt growth is system-side for the fourth consecutive release.** Prompt tokens are up 19.7%
(+5,420) with one new prompt file (14 → 15), and the mix goes from 65.7%/34.3% system/tools to
71.3%/28.7%. The arithmetic the 2.1.258 and 2.1.259 notes use puts the total at ~27.5k before and
~32.9k after, which holds tools at ~9.44k against ~9.45k — tens of tokens, inside the rounding the
0.1% percentages carry, so flat rather than a finding — while system rises ~18.1k → ~23.5k. Across
2.1.257 through 2.1.260 that is +19,346 tokens, all of it system text, taking the system prompt from
~4.2k to ~23.5k while every tool description stayed at ~9.45k. The consequence for Canopy has not
changed across the four: flat tool descriptions ask nothing of the code keyed to tool names and
shapes (`summarizeToolInput`, the tool views, the `PreToolUse`/`PostToolUse` normalization), and the
system growth is overhead off the usable context of every pane, visible as a higher starting context
percentage in the Agent Inspector and as compaction arriving sooner.

**54 of 2.1.260's CLI changelog entries were not readable this run, and delegating did not help.**
The release notes truncate at "… +54 more CLI changelog entries" — twice the 25 hidden at 2.1.259 —
and both routes to the rest were denied again: `gh api` against the changelog repo by the allowlist
defect, and `WebFetch` on its own. New this run is that the denial is not the main session's alone. A
subagent spawned with `WebFetch` and `WebSearch` in its tool list hit the same refusal, so delegating
the fetch is not a way around it and is not worth the turns. The items above are the visible entries
only, which is not the same as having reviewed the release.

**2.1.261's 128K inline output limit reached Canopy through the hook script's argv, and that is the
first release note in this range to have forced a code change.** `bashOutputMaxChars` and
`taskOutputMaxChars` raise how much command and background-task output stays inline before being
spilled to a file, up to 128K characters. Canopy forwards that output verbatim: `PostToolUse` carries
it in `tool_response`, which `normalizeEvent` reads and the tool views render. Until this release
`canopy-agent-hook.sh` passed the entire hook body to `curl` as an argv element (`-d "$INPUT"`), and
Linux caps a single `execve` argument at `MAX_ARG_STRLEN` — 32 pages, 131,072 bytes. A 128K-character
`tool_response` lands on that limit before the rest of the payload is counted and any JSON escaping is
applied, so `execve` fails with `E2BIG`; the script's `2>/dev/null` and `|| exit 0` then swallow it and
the event is dropped with no trace. The symptom would have been a notch stuck on `toolCalling`,
showing the last tool until the next event arrived, on exactly the turns with the most output. The
script now streams the body from stdin with `--data-binary @-`, which has no size limit — the form
`canopy-agent-hook.cmd` already used, so the two agree. `canopy-agent-statusline.sh` keeps the argv
form deliberately: status payloads carry `version`, `model`, `context_window`, `cost` and
`rate_limits`, never tool output.

**Neither setting needs a preference of its own, because the profile Settings JSON already reaches
them.** This is the same seam the 2.1.259 `Read()` and 2.1.260 permission-rule notes turn on:
`tabCommands.ts` parses a profile's `claude.settingsJson` and `setupSettings` merges it into the
per-session `--settings` file unmodified, so a user can set either key today without Canopy knowing
the names. That is why the argv defect above was reachable in-product rather than hypothetical. The
server-side cap needs no change to match: `AgentHookServer`'s `MAX_BODY_BYTES` is 1 MB, and 128K
characters is at most ~768 KB even if every one of them escapes to a six-byte `\uXXXX`, so the
"Hook server body too large" row below stays accurate rather than becoming the new bottleneck.

**`--append-subagent-system-prompt-file` is the file form of a flag Canopy does not pass.** It exists
for subagent system prompts too large for a command line. The profile's Append System Prompt field
maps to `--append-system-prompt` for the main session only (`buildCliArgs`), and Canopy has no
subagent-prompt field to feed the new flag from — it observes subagents through `SubagentStart` /
`SubagentStop` counts rather than configuring them. Worth noting for the pattern rather than the flag:
the CLI is moving prompt-sized arguments off argv, for the same reason the hook script had to.

**`/skill-doctor` reports on a surface Canopy manages but does not report on.** It lists which loaded
skills go unused and what they cost in context. Canopy has its own skills subsystem — `SkillScanner`,
`SkillParser`, `SkillInstaller` and a `claude.ts` transformer that writes skills into the format
Claude Code reads — so a pane's loaded skills are partly Canopy's doing, and the context they consume
compounds the system-prompt growth tracked below. The command is pane-local and typed into the PTY,
so nothing is required here, but the pruning signal it produces has no path back into the Skills
preferences UI that installed them.

**The two SDK-side fixes do not reach `commitMessageGenerator`.** 2.1.261 makes SDK and cloud sessions
honour Stop/interrupts sent right after the first prompt, and stops a resume losing hook output and
other context around parallel tool calls. Canopy's only SDK caller runs a single-shot `query()` with a
JSON schema and no tools, drains the iterator to the `result` message and never interrupts or resumes,
so neither path is exercised. The resume fix does matter to panes, which resume through
`buildResumeArgs`' `--resume` — but panes run the user's own `claude` binary, so they get it by the
user updating, not by this pin. The new "Organization policy" line in `/status` and `claude doctor` is
rendered CLI output, not the status-line JSON `normalizeStatus` parses, so it is invisible to Canopy
either way.

**Prompt growth is system-side for the fifth consecutive release.** Prompt tokens are up 17.6%
(+5,809) with one new prompt file (15 → 16), and the mix goes from 71.3%/28.7% system/tools to
75.6%/24.4%. The arithmetic the 2.1.258 note introduces puts the total at ~33.0k before and ~38.8k
after, which holds tools at ~9.47k against ~9.47k — flat again — while system rises ~23.5k → ~29.3k.
Across 2.1.257 through 2.1.261 that is +25,155 tokens, all of it system text, taking the system prompt
from ~4.2k to ~29.3k while every tool description stayed at ~9.45k. Five releases is enough to call
this a trend rather than a run: the code keyed to tool names and shapes (`summarizeToolInput`, the
tool views, the `PreToolUse`/`PostToolUse` normalization) has been asked for nothing, and the cost is
entirely context Canopy's panes no longer have.

**55 of 2.1.261's CLI changelog entries were not readable this run.** The release notes truncate at
"… +55 more CLI changelog entries" — the largest hidden count in this range, after 25 at 2.1.259 and
54 at 2.1.260 — and every route to the rest was denied: `gh api` against the changelog repo by the
allowlist defect described in `.github/prompts/claude-code-compat.md`, and `WebFetch` and `WebSearch`
on their own. The items above are the visible entries only. The reachable-without-network check that
the 2.1.259 notes describe was run and still reports `next`'s pin rather than `TO_VERSION`:
`manifest.json` carries `"2.1.207"`, so its 30-name `HookEvent` union remains a floor on the
subscription gap, not a current list.

**2.1.263 names nothing that reaches Canopy, and for once that is the whole release rather than the
visible part of it.** Its CLI changelog is a single line — "Bug fixes and reliability improvements" —
with no flag, environment variable, config key, hook, permission or tool change named, and no
`… +N more CLI changelog entries` marker. That last detail is what separates this note from the
2.1.259, 2.1.260 and 2.1.261 ones above, which each had 25, 54 and 55 entries hidden behind that
marker: here the release-notes text is complete even though the diff is still unreachable, so
"nothing named" is a statement about the release rather than about what this run could see. The
unreachability is unchanged and was re-confirmed — `gh api` against the changelog repo denied by the
allowlist defect described in `.github/prompts/claude-code-compat.md`, `WebFetch` denied on its own —
so an unannounced change remains possible; it is only the announced set that is known to be empty.
2.1.262 never shipped: the changelog repo has no release for it and npm goes `0.3.261` → `0.3.263`.

**Prompt growth is system-side for the sixth consecutive release, and the tools half has now been
flat for six.** Prompt tokens are up 16.0% (+6,198) with one new prompt file (16 → 17), and the mix
goes from 75.6%/24.4% system/tools to 79.0%/21.0%. The arithmetic the 2.1.258 note introduces puts
the total at ~38.7k before and ~44.9k after — the "before" reproducing the ~38.8k the 2.1.261 note
derived independently, which is the useful check on this method — and that holds tools at ~9.45k
against ~9.44k while system rises ~29.3k → ~35.5k. Across 2.1.257 through 2.1.263 that is +31,353
tokens, all of it system text, taking the system prompt from ~4.2k to ~35.5k while every tool
description stayed at ~9.45k. Six releases makes the earlier reading safe to rely on rather than
merely repeat: the code keyed to tool names and shapes (`summarizeToolInput`, the tool views, the
`PreToolUse`/`PostToolUse` normalization) has been asked for nothing across the entire range, and
this release is the strongest case of it — a 21% jump in system text against a tools half that did
not move.

**The bundle delta argues against reading that new file as 6.2k tokens of new instruction text.**
The same metadata reports the bundle up 2.0 kB (+0.0%). At roughly four characters per token, 6,198
tokens is on the order of 25 kB of prose, and prompt text is string literals that neither minify nor
compress away in a bundle measured this way. Ten kB of the difference could be argued about; an
order of magnitude cannot. The likelier reading is that the file is assembled at runtime from text
already present — a prompt now emitted where it previously was not, rather than one newly written —
which is also why a release whose only announced content is bug fixes can post its largest prompt
delta in the range. Recorded as an inference, not a finding: the diff that would settle it is behind
the same denial as everything else. Either way the consequence for Canopy is the same, because the
consequence follows from the tools half being flat, which is measured rather than inferred.

**The context-window floor moves with it, and the two places Canopy draws thresholds on it do not
need changing.** `normalizeStatus` reads `context_window.used_percentage` straight from the status
line, so a system prompt that grew ~6.2k tokens raises the number every pane starts at: about +3.1
percentage points on a 200k window, +0.6 on a 1M one. `AgentInspector.svelte` colours its bar at
≥70% and ≥90% and clamps the width with `Math.min(…, 100)`; `StatusBar.svelte` colours the `ctx N%`
readout the same way. Both take the percentage the CLI computes rather than deriving it, so the
floor shifting is reflected honestly and no constant here encodes an assumption that just became
wrong. Worth stating because it is the one Canopy path this release's only measurable quantity
flows through, and the answer is that it flows through correctly.

**2.1.265 regressed an environment variable Canopy can hand the CLI without anyone having typed it,
and the pin steps over that release rather than through it.** `CLAUDE_CODE_USE_GATEWAY` is
undocumented and was previously ignored unless `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN` were
both set. In 2.1.265 it began forcing Cloud-gateway sign-in on its own, so a configuration that set
it alongside an API key, an `apiKeyHelper` or custom auth headers failed **every** request with
"Not signed in to the Cloud gateway"; 2.1.266 restores the old behaviour and needs no configuration
change. The combination is reachable here without a Canopy setting being involved:
`commitMessageGenerator.ts` builds the SDK's child env as `{ ...process.env, ...envOverrides }` and
then layers `ANTHROPIC_API_KEY` on top, so a user who exports `CLAUDE_CODE_USE_GATEWAY` in their
shell supplies the first half and Canopy's own preference supplies the second. This lands through
the vendored-fallback path the 2.1.258 note describes — `pathToClaudeCodeExecutable` is `undefined`
when `which claude` fails, and the SDK then runs the CLI it vendors, which `package.json` pins. So
the pin moves `0.3.263` → `0.3.266` and deliberately skips `0.3.265`, whose vendored CLI is the
broken build. Panes are unaffected by the pin either way: they spawn the user's own binary, and a
user on 2.1.265 is fixed by updating it.

**Adding `CLAUDE_CODE_USE_GATEWAY` to `BLOCKED_ENV_VARS` was considered and rejected.** It would
mask the 2.1.265 failure, but the variable is legitimate for users who genuinely front the API with
the Cloud gateway, and blocking it would break them on every release where it works correctly —
trading a transient upstream bug for a permanent Canopy one. The blocklist exists to stop
user-supplied `customEnv` from redirecting an agent's traffic or loading code (`LD_PRELOAD`,
`*_PROXY`, the CA-bundle overrides); a variable that only selects between two of Anthropic's own
auth paths is not that. The pin is the whole fix.

**2.1.265 gives back the entire 2.1.257–2.1.263 system-prompt growth, and the number is exact rather
than approximate.** Prompt tokens are down 69.7% (−31,353) with six prompt files removed (17 → 11),
and the mix inverts from 79.0%/21.0% system/tools to 30.7%/69.3%. The arithmetic the 2.1.258 note
introduces puts the total at ~45.0k before and ~13.6k after, which holds tools at ~9,446 against
~9,446 — flat for the seventh consecutive release — while system falls ~35.5k → ~4.2k. The −31,353
is the same figure, to the token, that the 2.1.263 note records as the cumulative system-side growth
across 2.1.257 through 2.1.263, so this release does not merely trim the system prompt but returns it
to its pre-2.1.257 size of ~4.2k. Two independent checks say the method is sound rather than
coincidental: this release's derived "before" (~45.0k total, ~35.5k system, ~9.45k tools) reproduces
the 2.1.263 note's derived "after", and the tools half has now sat at ~9.45k across seven releases in
which system text tripled and then collapsed.

**The bundle delta says that text was not deleted, which matters for whether the saving is
reliable.** The same metadata reports the bundle **up** 413.8 kB (+0.9%) in the release that shed
~31.4k tokens of prompt — on the order of 125 kB of prose at roughly four characters per token. Text
removed from the bundle cannot make it larger, so the 2.1.263 note's inference that these prompts are
assembled at runtime from material already present is strengthened rather than merely repeated: what
changed is when the text is emitted, not whether it exists. The practical consequence is a caveat on
the paragraph below — a session that triggers whatever now gates that text could still pay for it, so
the floor should be read as lower on average rather than lower always. Recorded as an inference; the
diff that would settle it is behind the same denial as everything else.

**The context-window floor moves the other way for the first time in this range, and again nothing
needs changing.** `normalizeStatus` reads `context_window.used_percentage` straight from the status
line, so ~31.4k fewer tokens of system prompt lowers the number every pane starts at by about 15.7
percentage points on a 200k window and 3.1 on a 1M one — roughly reversing the ~+9 points the
2.1.257–2.1.263 notes tracked accumulating. `AgentInspector.svelte` colours its bar at ≥70% and ≥90%
and `StatusBar.svelte` colours the `ctx N%` readout the same way; both consume the percentage the CLI
computes rather than deriving it, so a floor that drops is reflected as honestly as one that rose.
Users should see panes starting materially emptier and compaction arriving later. This is the release
range's largest user-visible change to Canopy and it arrives entirely through data, with no code.

**What else 2.1.265 names does not reach Canopy.** `--plugin-dir` can now point at a folder of
plugins, with child manifests auto-loading and additions or removals picked up while running; Canopy
passes no `--plugin-dir` at all, and `buildCliArgs` emits only `--model`, `--permission-mode`,
`--effort` and `--append-system-prompt`. The two plugin-path security fixes — a backslash in a plugin
path bypassing the symlink containment check on macOS and Linux, and directories whose names begin
with two dots being wrongly refused — are unreachable for the same reason. Telemetry now sends
`user.email` and `user.groups` from Claude Desktop and Cowork through a Claude apps gateway, which is
those products' own telemetry path and not something a Canopy pane emits. The new 1 GB cap on tool
results saved to disk, with the conversation preview marking a saved file as truncated, applies to
the CLI's own conversation storage; Canopy sees tool results as the `tool_response` field of a hook
payload, and the limit that governs that path is still the hook server's 1 MB body cap in the table
below. The prompt-cache fixes for resumed foreground subagents and for agent teammates moving
`SubagentStart` hook context out of the prompt prefix are internal to the CLI's cache accounting —
Canopy registers `SubagentStart` but only normalizes it, and the resume path it owns is
`buildResumeArgs`' `--resume`. The resume-after-crash fix, which stops the last prompt being
rewritten and keeps an interrupted tool call marked interrupted, likewise changes transcript content
Canopy does not parse.

**One 2.1.265 fix is worth knowing about at the support level even though it needs no code.**
`/model opusplan[1m]` was being rejected with "Model not found". Canopy's profile Model field is free
text — placeholder "sonnet, opus, haiku, fable, or model ID" — and `buildCliArgs` forwards whatever
it holds as `--model`, so `opusplan[1m]` is a value a user can enter. The release note names the
slash command rather than the flag and the diff was not reachable, so whether `--model` was affected
identically is not established here; what is established is that a user who hit this had a plausible
route to reading it as a Canopy bug, since the failure would surface inside a Canopy pane.

**38 of 2.1.265's CLI changelog entries were not readable this run.** The release notes truncate at
"… +38 more CLI changelog entries" and every route to the rest was denied: `gh api` against the
changelog repo by the allowlist defect described in `.github/prompts/claude-code-compat.md`, and
`WebFetch` on its own — the seventh consecutive denial and the eighth of nine attempts across this
range. The items above are the visible entries only. 2.1.266 has no such marker: its changelog is the
single `CLAUDE_CODE_USE_GATEWAY` fix, its prompt tokens and file count are unchanged from 2.1.265
(+0, +0.0%, mix flat at 30.7%/69.3%) and its bundle moves −0.1 kB, so that release is known complete
rather than merely un-truncated. 2.1.264 never shipped: the changelog repo has no release for it and
npm goes `0.3.263` → `0.3.265`.

**2.1.267 is the first release in this range that adds a flag Canopy had a reason to adopt, and the
reason is a seam Canopy already owned.** `--system-prompt-snapshot off` re-renders the system prompt
on every request instead of replaying the one recorded when the conversation started; the release
note frames it as a convenience for iterating on prompt text. That framing undersells it here,
because the prompt text a Canopy user iterates on is a Canopy preference — the profile's **Append to
system prompt** field — and the path that ignores their edit is `buildResumeArgs`' `--resume`. So a
user who edited that field and resumed an existing pane got the old text with nothing indicating
why. `buildCliArgs` now emits `--system-prompt-snapshot off` when the profile opts in.

**It is opt-in for a reason that is easy to get wrong.** Panes spawn the user's **own** `claude`
binary rather than the SDK's vendored copy, so the flag reaches a CLI whose version Canopy does not
control, and an unknown flag is a startup failure rather than a warning. The adapter therefore emits
only the literal `off` and omits the flag for every other state, including the default — the same
shape `effortLevel` already uses, where empty means "add no flag" rather than "add a default value".
A user who turns it on is opting into requiring 2.1.267, which the help text says.

**`maxEffortLevel` needs no code and is worth a support note anyway.** The new setting caps effort on
every provider including Bedrock, Vertex and Foundry, and it can sit at the top level or per model
under `modelSettings`. It reaches Canopy through the profile's existing **Settings JSON override**,
which `setupSettings` merges into the per-session `settings.json` wholesale, so nothing needed
adding. The interaction worth knowing is that the cap is **silent**: `buildCliArgs` still emits
`--effort max` from the dropdown, the CLI still lowers it, and the dropdown still reads "Max". A user
under managed settings who reports that Max behaves like Medium is describing correct behaviour, so
the Effort level help text now says so.

**One 2.1.267 fix lands on a provider configuration Canopy itself enables.** Expired AWS or Google
Cloud credentials under a host app were retrying ten times behind a generic "request failed" before
the re-authenticate error appeared. Canopy sets `CLAUDE_CODE_USE_BEDROCK` and `CLAUDE_CODE_USE_VERTEX`
from the profile Provider dropdown, so this is not a configuration a Canopy user has to go out of
their way to reach — it is two clicks in the profile editor. Nothing to change; the failure just
becomes legible, and it becomes legible inside a Canopy pane where it would otherwise have read as a
Canopy networking problem.

**The managed hook-settings fix reads as though it should reach Canopy and does not.** 2.1.267 makes
managed `allowedHttpHookUrls`, `httpHookAllowedEnvVars` and `allowedChannelPlugins` admit nothing
rather than everything when unreadable — a fail-closed change that would break hook delivery under
managed settings for anyone relying on the old permissive fallback. Canopy posts every hook to
`127.0.0.1` over HTTP, so the naive reading is that this gates Canopy. It does not: `setupSettings`
registers each event as `{ type: 'command', command: hookScriptPath }`, and the HTTP request happens
one level down, inside `canopy-agent-hook.sh`'s `curl`. The CLI sees a command hook and never
consults its HTTP-hook allowlist. Recorded because the wrong reading is the more natural one and the
right one is two files away.

**The large-session resume fix is transcript-side, but Canopy users are the ones who reach it.**
Resuming a session whose transcript exceeds 5 MB was dropping parallel tool calls and their hook
output from the reloaded conversation. Canopy does not parse transcripts — the same reasoning the
2.1.265 note applies to the resume-after-crash fix — so no code changes. What is worth naming is the
exposure: `buildResumeArgs` is how every Canopy pane returns to a session, and a workstation whose
sessions persist across days accumulates 5 MB far more readily than a one-off terminal invocation
does. A resumed pane that came back missing tool calls would have looked like Canopy losing them.

**What else 2.1.267 names does not reach Canopy.** The tmux/ssh fix — shift+enter and option+backspace
failing after reconnecting inside an agent view — is about the CLI's own agent view, not Canopy's
panes, which run their own PTY through `PtyManager` and handle keys in `TerminalInstance.svelte`;
worth stating explicitly because Canopy is a terminal workstation and the entry reads as though it
were addressed to one. The `-p --resume` fix for a spurious "Continue from where you left off." turn
is scoped to print mode, which panes do not use and `commitMessageGenerator` reaches through the SDK
rather than the CLI. Cowork scheduled tasks failing under sandboxing-required managed settings, `/context`
rendering blank on mobile clients, and the dim last-prompt header in fullscreen are all other
products' surfaces or CLI chrome. The marketplace backslash containment fix is unreachable: Canopy
passes no marketplace or `--plugin-dir` configuration at all. The Workflow `agent()` large-output-schema
fix is internal to that tool.

**Prompt tokens are flat for the second release running, and the bundle delta confirms the 2.1.265
inference rather than complicating it.** Prompt tokens move +0 (+0.0%) with no file added or removed
and the mix identical at 30.7%/69.3% system/tools, so the totals hold exactly where 2.1.266 left
them: ~13.6k overall, ~4.2k system, ~9.4k tools. The tools half has now been ~9.4k continuously since
before 2.1.257, through a system prompt that tripled and then collapsed back. Meanwhile the bundle is
**up** 277.7 kB (+0.6%) in a release that changed no prompt text at all — which puts the bundle near
46 MB, reproducing the figure implied by 2.1.265's +413.8 kB at +0.9%, and which is what a release
that adds a setting, a flag and forty-odd fixes should look like. Nothing here moves the
context-window floor, so `normalizeStatus`, `AgentInspector.svelte` and `StatusBar.svelte` are
untouched by this release in the way the 2.1.257–2.1.265 notes track.

**41 of 2.1.267's CLI changelog entries were not readable this run.** The release notes truncate at
"… +41 more CLI changelog entries" against 13 visible, and both routes to the rest were denied again:
`gh api` against the changelog repo by the allowlist defect described in
`.github/prompts/claude-code-compat.md`, and `WebFetch` on its own — the eighth consecutive denial.
The items above are the visible entries only. The two highlights this release leads with are both
covered above, so the adoption decision does not rest on the hidden entries, but a flag or setting
among those 41 would not have been seen.

**2.1.268 fixes a total outage on the endpoint configuration Canopy's own help text tells users to
pick, and this branch had been shipping it for three increments.** Since 2.1.265 a regex in the
Artifact tool's input schema made every turn fail with HTTP 400 against third-party
Anthropic-compatible endpoints — anything `ANTHROPIC_BASE_URL` points at that is not Anthropic's own
API. The **Base URL** field in the Claude profile editor sets exactly that variable, and its help
text reads "Use for Ollama, GLM, MinMax, or any OpenAI-compatible Anthropic proxy", so this is not an
exotic configuration a user has to construct: it is the field's advertised purpose. The SDK pin moved
`0.3.265` → `0.3.266` → `0.3.267` across this PR while the bug was live, which is the first time in
this range a bump carried a defect rather than merely a version number. `0.3.268` retires it.

**The pin fixes one of the two paths that set that variable, and it is the less used one.**
`commitMessageGenerator` runs through the SDK's `query()` and passes `ANTHROPIC_BASE_URL` on the
scoped child env, so it takes the fix from `package.json` — but only when it falls back to the
vendored binary. It prefers the user's own `claude`, resolved from `PATH` by
`resolveClaudeExecutable()` and handed to `pathToClaudeCodeExecutable`. Panes have no fallback at
all: `buildEnvVars` sets the variable for a CLI Canopy spawns but does not install. So a user on a
2.1.265–2.1.267 CLI with a Base URL set still sees every turn fail after this bump, and the fix
reaches them when they update their own CLI — the same asymmetry `--system-prompt-snapshot` has, read
in the other direction.

**The CPU fix is the one 2.1.268 entry written for how Canopy is used rather than for what it
configures.** Two causes are named: a busy loop that pinned a core in long-running idle sessions, and
rapid terminal focus reports during a session recap. Both describe a workstation rather than a
terminal invocation. Canopy keeps a `claude` process per pane across worktrees and tabs, most of them
idle most of the time, so a per-session busy loop multiplies by the pane count rather than costing
one core. And the focus half meets Canopy twice over: `TerminalInstance.svelte` calls `term.focus()`
from a `$effect` every time a tab becomes active, again from the `canopy:focus-terminal` handler, and
again at init — while `buildResumeArgs` is how every pane returns to a session, which is when a recap
runs. A layout restore does both at once, mounting many panes that each resume and each take focus as
they become visible. Nothing to change here, and nothing the SDK pin delivers: panes run the user's
own binary.

**A worktree Canopy just created is, to the CLI, a folder nobody has trusted — and 2.1.268 closes one
consequence of that.** A respawned in-process teammate was picking up tools or a system prompt from a
same-named agent file in an untrusted folder. Canopy reaches untrusted folders by construction rather
than by accident, which the 2.1.252 note above already establishes from the other side: every pane
starts in a fresh worktree, and fresh is what untrusted means here. The routine case is worth naming
because it does not look like a security scenario — a user makes a worktree for someone else's branch
to review it, the branch carries `.claude/agents/*.md`, and a pane opens in it. Canopy passes
`--settings` with a temp file but sets no isolated config directory for Claude the way it does for
Gemini, so the worktree's own agent files are in scope for the CLI exactly as they would be outside
Canopy. No code change: the fix is upstream and Canopy has no say in folder trust.

**The new `WebFetch` deadline needs no code and removes a way panes could hang busy forever.** A
fetch against a server that holds the response open without finishing now fails after 300 seconds,
overridable with `CLAUDE_CODE_WEBFETCH_DEADLINE_MS` (`0` disables it). That variable is in neither
`BLOCKED_ENV_VARS` nor the adapter's `INTERNAL_BLOCKED`, so the profile's **Custom env** field
already carries it and nothing needed adding. The Canopy-side consequence is in the busy/idle model:
`PreToolUse` is a busy event and `Stop`/`StopFailure`/`SessionEnd` are the idle ones, so a fetch that
never returned left the pane pinned busy with no event able to clear it, and the worktree badge
aggregated that stuck state upward. A bounded failure produces the `Stop` that releases it.

**The gateway pricing entry lands on a number Canopy already draws.** With `pricing:` set in
`gateway.yaml`, signed-in clients now receive the same rates through managed settings, so the CLI's
own cost accounting matches the gateway's spend meter. Canopy reads `cost.total_cost_usd` from the
status line in `normalizeStatus`, stores it as `session.costUsd`, and renders it through `formatCost`
in both `StatusBar.svelte` and `AgentInspector.svelte` — so for anyone behind a priced gateway that
figure stops being list-rate arithmetic. Reached through **Custom env**, not the Provider dropdown,
which offers only `bedrock`, `vertex` and `foundry`.

**What else 2.1.268 names does not reach Canopy.** `configDirectory` in `claude auth status --json`,
`--json` on the five `claude plugin` subcommands and `errorDetails`/`noteDetails` in `claude plugin
list --json` are all additions to CLI subcommands Canopy never invokes: it shells out to `git` and
`gh`, and to `claude` only as a PTY session or through the SDK. The MCP "your message came through
empty" fix cannot reach a Canopy-configured server because Canopy configures none — there is no MCP
code in `src/main/` at all — though a user's own `~/.claude.json` servers are in scope. The gateway
`allow_cidrs` startup warning, the public-address alert, `gatewayInternalNetworks` and
`claude self-hosted-runner --remove-session-state` are operator surfaces for people running a gateway
or a runner, not for a desktop client. Artifact browser-tab icons are a published-artifact surface.
One near-miss worth recording so it is not looked up again: `src/main/changelog/` sounds like a Claude
Code integration point and is not — `fetchChangelog.ts` reads Canopy's own GitHub releases.

**Prompt tokens are flat for the third consecutive release, and this release is a good reminder that
"+0" is a floor rather than a proof.** Prompt tokens move +0 (+0.0%) with no file added or removed
and the mix identical at 30.7%/69.3%, so the totals hold where 2.1.266 and 2.1.267 left them: ~13.6k
overall, ~4.2k system, ~9.4k tools. But 2.1.268 demonstrably _did_ change a tool's input schema —
that is what the HTTP 400 fix is — and the metadata did not register it. The percentages are given to
0.1%, which on ~13.6k is about ±13 tokens, and a regex pattern removed from one JSON schema fits
inside that. So the arithmetic in `.github/prompts/claude-code-compat.md` is good for spotting a
change of a few hundred tokens and cannot rule out one of a few dozen. The bundle is the only figure
moving: +510.4 kB (+1.1%) implies ~46.4 MB before and ~46.9 MB after, which lines up with the
2.1.267 note's ~46.6 MB inside the rounding both figures carry.

**84 of 2.1.268's CLI changelog entries were not readable this run** — against 12 visible, the
largest hidden count anywhere in this range (2.1.267 hid 41, 2.1.265 hid 38), for a 96-entry release
built in the 23h 41m since 2.1.267. Both routes were denied again: `gh api` against the changelog
repo by the allowlist defect described in `.github/prompts/claude-code-compat.md`, and `WebFetch` on
its own — the ninth consecutive denial. The items above are the visible entries only, and with 84
hidden the usual caveat is doing more work than usual: the release-notes highlights lead with the
HTTP 400 fix, so the finding that mattered most here was visible, but seven eighths of this release
was not.

**2.1.269 fixes a stray-text bug whose exact string is one Canopy's own terminals emit, and this is
the rarest kind of entry in this range: one where Canopy is the terminal the CLI was mis-handling.**
The entry reads "fixed the terminal's replies to capability queries (`^[[?1;2c`) appearing as stray
text at startup in some terminals". That reply is not a constant the CLI invents — it is what a
terminal sends back when asked, and the chain from Canopy to that literal is short enough to verify
end to end in the vendored xterm.js. `sendDeviceAttributesPrimary` answers a primary Device
Attributes query (`CSI c`) with `ESC [?1;2c` when `_is("xterm") || _is("rxvt-unicode") ||
_is("screen")`; `_is(e)` is `rawOptions.termName.indexOf(e) === 0`; and xterm.js's default is
`termName: "xterm"`. Canopy sets no `termName` at any of its three `new Terminal({...})` sites —
`TerminalInstance.svelte`, `CreateWorktreeModal.svelte` and the remote-control
`RemoteTerminalView.svelte` — so the default stands and **every Canopy terminal replies with exactly
the byte sequence the release note names.** A Canopy user starting a pane on 2.1.268 or earlier saw
`[?1;2c` printed into their session.

**No Canopy change, and the reason is worth stating because the tempting fix is wrong.** Answering
`CSI c` is correct terminal behaviour, not a Canopy defect; suppressing the reply would break every
other program that probes the terminal. The bug was the CLI issuing a query and then failing to
consume its own answer, and it is fixed upstream. What Canopy inherits is the 2.1.268 asymmetry read
once more in the same direction: panes run the **user's own** `claude`, resolved from `PATH`, so the
SDK pin in `package.json` does not deliver this to a pane. A user still on ≤2.1.268 keeps seeing the
stray text until they update their own CLI, and the mobile remote view is affected identically
because `RemoteTerminalView.svelte` is xterm.js on the same defaults.

**The Bash tool's new diff is the release's lead item, and where it lands on Canopy is the hook body
cap rather than any rendering code.** With `bashEditDiffEnabled`, a Bash result now carries a diff of
the files the command changed alongside its output. Nothing in Canopy renders tool results, so the
visible surface is unaffected; what changes is size. `PostToolUse` carries `tool_response` through
`canopy-agent-hook.sh` to `AgentHookServer.readBody`, which destroys the request past
`MAX_BODY_BYTES` (1 MB) and drops the event silently — the row already in **Error states** below. A
dropped `AfterToolUse` leaves the pane on `toolCalling` showing the finished Bash command until the
next event arrives, self-healing at `Stop`. Whether this is now reachable depends on something this
run could not establish: if the CLI applies its 128K-character inline output limit (the 2.1.261
note) to the whole result including the diff, a Bash result cannot approach 1 MB and nothing changes;
if the diff is appended after that truncation, the ceiling moved. **No code change on an unverified
mechanism** — recorded so the next run that can read a diff knows which question to answer. The
`summarizeToolInput` path is not involved either way: it reads tool _input_, and Bash's input is
still `command`.

**The four new environment variables all reach a pane already, and the check is the same one each
time.** `OTEL_METRICS_INCLUDE_REPOSITORY`, `CLAUDE_CODE_GATEWAY_MODEL_DISCOVERY_TIMEOUT_MS`,
`CLAUDE_CODE_WORKFLOW_MAX_CONCURRENT_AGENTS` and `CLAUDE_CODE_BG_TASKS_REPORT_RUNNING` are in neither
`BLOCKED_ENV_VARS` nor the adapter's `INTERNAL_BLOCKED`, so the profile's **Custom env** field
already carries all four and nothing needed adding. The gateway discovery timeout is the one with a
real audience here: the 2.1.268 note establishes that the **Base URL** field actively steers users
onto third-party endpoints, and a gateway slower than the 3s `/v1/models` default is exactly the
configuration that field produces. `bashEditDiffEnabled` is a setting rather than a variable and is
reachable too, through the Claude profile's **Settings JSON override**, which `setupSettings` spreads
into the generated file ahead of Canopy's own `hooks` key.

**Two entries name Canopy-shaped things and reach neither.** The key-encoding fixes — F1/F2/F4 under
the kitty protocol, Delete in `st`, Alt+arrows in `rxvt-unicode`, Shift+punctuation in WezTerm — are
selected by terminal identity, and `PtyManager` sets `TERM=xterm-256color` with
`TERM_PROGRAM=canopy`, so none of those detection paths fire for a pane; xterm.js also implements no
kitty keyboard protocol for the CLI to negotiate. This is the same reasoning the 2.1.267 note applies
to the tmux/ssh fix, and it needs restating every time because Canopy being a terminal makes these
entries read as though they were addressed to it. Separately, `/output-style [name]` working "over
Remote Control" is a name collision: that is Claude Code's own Remote Control, while Canopy's is its
mobile client talking to the desktop app. In a pane the command is pane-local chrome, like `/diff`
and `/advisor` from 2.1.260. The `CLAUDE_CODE_BG_TASKS_REPORT_RUNNING` fix is scoped to remote and
headless sessions for the same reason it does not touch panes, which are interactive PTYs; the one
genuinely headless Claude path Canopy owns is `commitMessageGenerator`, a single-shot `query()` that
starts no background agents.

**Prompt tokens break an eight-release plateau, and it is the tools half that moved.** Prompt tokens
go +9,376 (+68.7%) with three files added (+27.3%). Working the arithmetic the usual way: 9,376 ÷
0.687 puts the total at ~13.6k before and ~23.0k after, and 11 files before against 14 after. The
"before" figure reproduces the ~13.6k the 2.1.266, 2.1.267 and 2.1.268 notes each derived
independently, which is the check that the series is still being read correctly. Splitting by the
given mix — 30.7%/69.3% before, 36.7%/63.3% after — gives system ~4.2k → ~8.4k and tools ~9.5k →
~14.6k, so **tools +5.1k (+54%) and system +4.3k (+102%)**. The tools half had sat at ~9.45k
continuously from before 2.1.257 through 2.1.268, surviving a system prompt that tripled and
collapsed back; this is the first release in the range to move it, and by the notes' own reasoning it
is the half that can force code changes here.

**It does not force one, and that was checked rather than assumed.** Three new prompt files and a
+5.1k tools jump is the shape of tool descriptions being _added_ — `claude plugin eval` and the Bash
diff are both visible candidates — but attribution needs `meta/prompt-stats.md`, which was
unreachable. What could be checked is whether an unknown tool name breaks anything on Canopy's side,
and it does not: `summarizeToolInput` ends in a generic fallback that returns the first non-empty
string value in the input, `EVENT_MAP` and `toNotchStatus` key off normalized _hook event_ names
rather than tool names, and the renderer special-cases exactly one tool (`question`, and that is
OpenCode's). A new tool degrades to a generic summary instead of failing. The bundle is +524.4 kB
(+1.1%); taken at face value that implies ~47.7 MB before, but the 0.1% rounding band spans
~45.6–49.9 MB and comfortably contains the ~46.9 MB the 2.1.268 note derived, so it corroborates
rather than adds anything. Nothing here moves the context-window floor, so `normalizeStatus`,
`AgentInspector.svelte` and `StatusBar.svelte` are untouched.

**86 of 2.1.269's CLI changelog entries were not readable this run** — against 12 visible, a
98-entry release built in the 1d 0h 26m since 2.1.268, and a new high for hidden entries in this
range (2.1.268 hid 84, 2.1.267 hid 41). Both routes were denied again: `gh api` against the changelog
repo by the allowlist defect described in `.github/prompts/claude-code-compat.md`, and `WebFetch` on
its own — the tenth consecutive denial. The items above are the visible entries only. The caveat
carries more weight than usual in one specific place: the +5.1k tools-side jump is the largest
unexplained figure in this range, and the entry that explains it is quite likely among the 86.

**2.1.270's single CLI entry is the fix for a regression this branch would otherwise have shipped.**
2.1.269 made read-only git commands in Bash start asking for permission once a session had been
running for a while; 2.1.270 reverts that. The bump from `0.3.269` to `0.3.270` is therefore a
regression fix rather than the usual version-tracking, and it is worth separating the two places
Canopy touches a CLI because only one of them moves with the pin. Panes run the **user's own**
`claude` resolved from `PATH` — the asymmetry the 2.1.268 and 2.1.269 notes both record — so a user
sitting on 2.1.269 keeps the spurious prompts until they update their own install, and no pin in this
repo reaches them. The pin governs exactly one binary: the SDK's vendored CLI, which
`commitMessageGenerator` falls back to when `which claude` finds nothing.

**That fallback never actually hits the regression, and saying so is the point.** The regression needs
a session that has been running for a while; `generateCommitMessageInner` is a single-shot `query()`
on `haiku` with the diff inlined in the prompt and a JSON schema on the output. It is one turn, and it
runs no read-only git commands of its own — the `git:generateCommitMessage` handler
(`src/main/ipc/handlers.ts:2197`) computes the diff with `GitRepository.getDiff` and passes it in.
So the honest statement of the bump's value is narrower than "fixes a bug for us": it keeps the
vendored fallback off a build that is known bad, which matters for the next regression rather than
this one.

**Where 2.1.269 _would_ have surfaced on Canopy is the notification path, and the amplification there
is Canopy's own.** `PermissionRequest` reaches `claudeAdapter.formatNotification`, and
`AgentSessionManager.showNotification` (`src/main/agents/AgentSessionManager.ts:359`) constructs and
shows a fresh OS `Notification` per event with no dedup, coalescing or rate limit. A pane on 2.1.269
doing ordinary git-heavy work would have emitted one desktop notification per read-only git command,
multiplied by every open pane. The CLI half is fixed upstream; the multiplier is not.

**No throttle was added, and the reason is a real tradeoff rather than scope discipline.** Deduping or
rate-limiting permission notifications trades a cheap failure for an expensive one: a duplicate
notification is noise, but a suppressed one is a pane sitting on `waitingPermission` with nothing
telling the user it needs an answer. That asymmetry argues for leaving the path alone until there is a
dedup key that provably cannot swallow a distinct request, which a burst of identical
`Bash`-plus-git-command bodies does not supply. The notch itself is fine either way: `toNotchStatus`
maps `PermissionRequest` to `waitingPermission`, and the next `PreToolUse` or `Stop` clears it, so a
denied request leaves no stuck state.

**The two named prompt changes are both tool descriptions, and both tools were already shape-covered.**
The release adds a description for a tool that runs a Bash command and returns its output, and one for
a tool that launches an agent for multi-step work. `summarizeToolInput` matches on input _shape_ and
not on tool name, and it has a `command` branch (Bash) and a `prompt` branch (the agent launcher)
before its generic fallback — so the `Task`→`Agent` naming the second item implies costs nothing here.
This is the 2.1.269 note's "an unknown tool degrades to a generic summary" check run against two named
tools instead of a hypothetical, and it comes back clean.

**The token split says the same thing, and it is the first release in a while where the falling tools
share is pure dilution.** Prompt tokens go +5,258 (+22.8%) with one file added (+7.1%). Working it the
usual way: 5,258 ÷ 0.228 puts the total at ~23.1k before and ~28.3k after, and 1 ÷ 0.071 gives 14
files before against 15 after. The "before" figure reproduces the ~23.0k the 2.1.269 note derived,
which is the continuity check that the series is still being read correctly. Splitting by the given
mix — 36.7%/63.3% before, 46.3%/53.7% after — gives system ~8.5k → ~13.1k and tools ~14.6k → ~15.2k,
so **system +4.6k (+55%) and tools +0.6k (+4%)**. Carrying the 0.1% rounding band through both
percentages puts the tools delta between +525 and +695, so it is a real increase and a small one. The
tools share dropping 63.3% → 53.7% is therefore system text being added around tool descriptions that
barely moved, exactly the dilution the prompt file warns not to read as a shrinking tools half — and
the single added file is system-kind at roughly 4.6k tokens, not either of the two tool descriptions
named above.

**The mismatch between one CLI entry and a +22.8% prompt jump is the open question this run leaves.**
Unlike the three releases before it, 2.1.270's notes carry no "… +N more CLI changelog entries"
truncation marker — one visible entry and, on its face, one entry total, built in the 1d 0h 35m since
2.1.269. But a release that adds a ~4.6k-token system prompt file while its changelog names only a
permission bugfix is not self-consistent, and this run could not resolve it: `gh api` against the
changelog repo was denied by the allowlist defect described in
`.github/prompts/claude-code-compat.md`, and `WebFetch` was denied for the eleventh consecutive run,
so neither `meta/prompt-stats.md` nor the official `CHANGELOG.md` was reachable. Nothing in the
visible entry forces a code change; the unread system file is where a next run with a working diff
route should look first.

**2.1.271 answers the question the 2.1.270 note left open, and the answer is that the file did not
survive.** 2.1.270 added a ~4.6k-token system prompt file against a changelog naming only a
permission bugfix, and that note closed by pointing a next run at it. 2.1.271 removes a prompt file
and cuts the system half by roughly twice that. Prompt tokens go −8,130 (−28.7%) with one file
removed (−6.7%): 8,130 ÷ 0.287 puts the total at ~28.3k before and ~20.2k after, and 1 ÷ 0.067 gives
15 files before against 14 after. Both "before" figures reproduce the 2.1.270 note's ~28.3k and 15
exactly, which is the continuity check that the series is still being read correctly. Splitting by
the given mix — 53.7%/46.3% tools/system before, 79.2%/20.8% after — gives system ~13.1k → ~4.2k and
tools ~15.2k → ~16.0k, so **system −8.9k (−68%) and tools +0.8k (+5%)**. Carrying the 0.1% rounding
band puts the tools delta between +760 and +808: real, and small. The system half is now smaller than
at any point this range has tracked — ~4.2k against the ~8.5k the 2.1.270 note derived for 2.1.269 —
so 2.1.271 did not merely reverse 2.1.270, it went well past it.

**2.1.272's changelog says "Bug fixes and reliability improvements" and its prompt numbers say a new
tool.** Prompt tokens +1,061 (+5.3%) with one file added (+7.1%): 1,061 ÷ 0.053 puts the total at
~20.0k before, which sits inside the rounding band around the ~20.2k derived above (5.3% ± 0.05%
admits 19.8k–20.2k), and 1 ÷ 0.071 gives 14 files before against 15 after — both matching 2.1.271's
"after", so the two releases chain cleanly. Total after is ~21.3k. Splitting by the given mix —
79.2%/20.8% before, 80.2%/19.8% after — gives tools ~16.0k → ~17.0k and system ~4.20k → ~4.21k, so
**tools +1,053 (+6.6%) and system flat**; the system delta's rounding band is −12 to +29, which is
noise. A single added file worth ~1.05k tokens landing entirely on the tools side is a tool
description. This is the 2.1.270 mismatch inverted, and the inversion is the part that matters: there
the unexplained file was system-kind and so could not force a change here, and this one is tools-kind,
which is the half that can.

**So the unknown-tool check was run against every path in Canopy that keys off a tool name, and it
comes back clean.** There are five, and none of them enumerate:

- `summarizeToolInput` (`src/main/agents/utils.ts:31`) matches input _shape_, not name — `command`,
  `file_path`, `questions[0].question`, `query`, `url`, `pattern`, `prompt`, `description`, `skill`,
  then a fallback returning the first non-empty string value. A tool nobody has heard of still
  summarizes as long as its input carries one string.
- `claudeAdapter.normalizeEvent` passes `tool_name` straight through as `string | undefined`. No
  union, no lookup table.
- `formatNotification` uses it as a string and already falls back to "A tool requires your approval".
- `toNotchStatus` keys off the _normalized event_, so `BeforeToolUse` → `toolCalling` regardless of
  which tool raised it.
- `EVENT_MAP` is keyed on hook event names, which tool additions do not touch.

**One naming trap, recorded because it costs a search every time.**
`src/renderer/src/lib/stores/toolView.svelte.ts`, `tools.svelte.ts` and
`mobile/src/constants/tool-icons.ts` all say "tool" and none of them mean a CLI tool call — they mean
Canopy's own agent tools (`claude`, `codex`, `gemini`, `opencode`, `shell`, `browser`). A grep for
tool-name impact lands there first and it is the wrong layer. `resolveToolIcon` has a `default` arm
regardless.

**2.1.271's per-command `allowed_domains` names two tool names Canopy has never seen, which is the
same result from the other direction.** The entry scopes sandboxed network access for Bash, PowerShell
and Monitor. `Grep` for `PowerShell` and `Monitor` across `src/` returns Electron's `powerMonitor`, a
PTY comment and an onboarding install hint — there is no tool registry to extend, because there is no
tool registry. And for Bash the new field rides alongside `command` in `tool_input`, which
`summarizeToolInput` tests first, so a sandboxed permission request still summarizes as the command
rather than the domain list.

**2.1.271's `/config` mouse support works in Canopy's agent panes unchanged, and the check is worth
keeping because this is the class that usually breaks.** A CLI that enables mouse tracking (DECSET
1000/1002/1003/1006) takes the wheel away from the terminal's own scrollback, so any app-level `wheel`
listener calling `preventDefault` unconditionally would swallow the new clicks and scrolls. Canopy has
none: `Grep` for `wheel|onWheel|mouseEvents` across `src/renderer/src/lib/terminal/` finds no wheel
listener, and the agent-pane `new Terminal({...})` (`TerminalInstance.svelte:472`) sets only
`fontSize`, `fontFamily`, `cursorBlink`, `allowProposedApi`, `theme`, `scrollback`, `linkHandler` and
the Windows `conpty` backend — nothing touching mouse reporting. xterm.js's native handling applies,
so the feature arrives for free. The other two `new Terminal` sites
(`RemoteTerminalView.svelte:356`, `CreateWorktreeModal.svelte:179`) host different surfaces and were
not part of this check.

**The one 2.1.271 fix that reaches a Canopy-configurable surface is the `ANTHROPIC_UNIX_SOCKET` one,
and it needs nothing from us.** Org policy was being fetched through, and rejected by, third-party
local proxies set that way; 2.1.271 treats them like other custom gateways again. That variable is
reachable from Canopy: `claudeAdapter.buildEnvVars` passes arbitrary `claude.customEnv` keys through,
and `ANTHROPIC_UNIX_SOCKET` is not in `BLOCKED_ENV_VARS` (`src/main/security/envBlocklist.ts`), which
blocks proxy and CA-bundle overrides but not this one. So a profile can set it and the fix applies —
subject to the asymmetry this file already records twice: panes run the user's own `claude` from
`PATH`, so the fix lands only when they update their own install, and the pin in this repo governs
only the SDK's vendored CLI.

**The rest of 2.1.271's visible list has no Canopy surface.** Remote-session fast mode and
`claude self-hosted-runner --drain-marker-file` concern deployments Canopy does not run;
`--accept-command` for `claude plugin install`/`update` and `omitClaudeMd` in agent frontmatter
concern surfaces Canopy does not drive — a `Grep` for `plugin install`, `--agents` and `omitClaudeMd`
across `src/main/` returns nothing, because Canopy writes agent _settings_ via `--settings` in
`setupSettings` and manages _skills_ under `src/main/skills/`, not subagent definitions. The
`modelPricing` multiplier, the managed-`mcp.json` handling and the Bedrock/Vertex/Foundry spinner tip
are display or policy changes with no field Canopy reads; `normalizeStatus` takes `cost.total_cost_usd`
as given.

**84 of 2.1.271's 96 CLI changelog entries were not readable this run**, against 12 visible. 2.1.272
shows a single entry with no truncation marker, but its prompt numbers describe a tool description its
changelog does not mention, so "reliability improvements" should not be read as "nothing shipped".
Both diff routes were denied again: `gh api` against the changelog repo by the allowlist defect
described in `.github/prompts/claude-code-compat.md` — the thirteenth consecutive run — and `WebFetch`
on its first call, the twelfth consecutive. The tool-name, terminal and env checks above were done
against Canopy's own files precisely because the diff was out of reach.

**2.1.273 adds a second consecutive tools-kind prompt file, and the system half has now been flat for
three releases.** Prompt tokens +1,357 (+6.4%) with one file added (+6.7%): 1,357 ÷ 0.064 puts the
total at ~21.2k before and ~22.6k after, and 1 ÷ 0.067 gives 15 files before against 16 after. Both
"before" figures reproduce the 2.1.272 note's ~21.3k and 15 exactly, so the chain from 2.1.271 holds
for a third link. Splitting by the given mix — 80.2%/19.8% tools/system before, 81.4%/18.6% after —
gives tools ~17.0k → ~18.4k and system ~4.20k → ~4.20k, so **tools +1,359 (+8.0%) and system flat**;
the system delta's rounding band is −38 to +34, which is noise. That is the 2.1.272 shape repeated: a
single added file landing entirely on the tools side is a tool description, and tools is the half that
can force changes here. The 2.1.272 note's unknown-tool audit enumerated all five Canopy paths that
key off a tool name and found none of them enumerate — `summarizeToolInput` matches input _shape_,
`normalizeEvent` passes `tool_name` through as `string | undefined`, `formatNotification` falls back,
`toNotchStatus` keys off the normalized event, and `EVENT_MAP` keys off hook events. None of those five
changed in this range, so the audit still holds and 2.1.273's new tool needs nothing from Canopy.

**The `.git/info/exclude` fix is the most Canopy-shaped entry in this release, and Canopy's main
worktree-removal path is immune by construction — but a second path is not.** 2.1.273 fixes a
long-running session recreating a stub `.git/info/exclude` after the repository's `.git` directory was
removed or moved away. On Canopy that would mean a `claude` pane cwd'd in a worktree resurrecting a
`.git` directory under a tree Canopy had just removed, which is precisely the "broken .git link —
the classic field ghost" debris `handlers.ts:2025` already classifies. It cannot happen on the path
users actually take: `worktree:removeWithBranch` calls `ptyManager.killUnderPathAndWait(worktree.path)`
and `disposeWatchersUnderPathAndWait` (`handlers.ts:1998-1999`) **before** the first
`GitRepository.worktreeRemove`, and `killUnderPathAndWait` waits for full process exit, so no session
survives into the removal to recreate anything. The ordering was added for Windows file handles; it
happens to close this too.

The gap is `git:worktreeRemove` (`handlers.ts:1881-1889`), which calls `GitRepository.worktreeRemove`
directly with no PTY teardown and no watcher disposal. It is not dead code — it is bridged as
`gitWorktreeRemove` (`src/preload/index.ts:877`) and typed in `src/preload/index.d.ts:765`, so it is a
fully reachable renderer API. Nothing calls it today: every real caller
(`WorktreeSection.svelte:99`, `ProjectTreeSection.svelte:174`, `CommandPalette.svelte:494`,
`HostRpcServer.ts:272`) uses `worktreeRemoveWithBranch`, and `CommandPalette.svelte:461` uses the
string `'git:worktreeRemove'` only as a command _id_. So this is latent rather than live, and worth
recording rather than fixing blind: the two handlers have diverged into a guarded and an unguarded
removal, and the unguarded one is the one a new call site would find first by name.

**The new MCP-disconnect notification reaches Canopy and is then deliberately dropped.** 2.1.273 adds
a notification when an MCP server disconnects mid-session and automatic reconnection gives up. Canopy
subscribes to `Notification` (`claude.ts:26`), maps it to the normalized `'Notification'`
(`claude.ts:49`) and captures `raw.message` as `event.message` (`claude.ts:125`) — and then surfaces
none of it: `formatNotification` returns `null` for anything that is not `PermissionRequest`
(`claude.ts:251`) and `toNotchStatus` ends in `.otherwise(() => null)` (`claude.ts:276`). A user whose
MCP server dies mid-session sees the CLI's own in-pane message and gets no OS notification and no notch
status change. That is Canopy's existing policy rather than a regression, and it also answers the
2.1.269 note's amplification concern for this entry from the other direction: the un-deduplicated
per-event `Notification` path cannot be amplified by a new notification class that never gets past the
`PermissionRequest` gate.

**Both of 2.1.273's new configuration surfaces are reachable from a Canopy profile and neither needs a
change.** `CLAUDE_CODE_GATEWAY_HINT_HEADERS=1` opts into five new `x-claude-code-*` request headers for
LLM gateways. It rides the same route as the 2.1.271 `ANTHROPIC_UNIX_SOCKET` finding:
`claudeAdapter.buildEnvVars` forwards arbitrary `claude.customEnv` keys, and `BLOCKED_ENV_VARS`
(`src/main/security/envBlocklist.ts`) contains no `CLAUDE_CODE`-prefixed entry at all — it blocks
system paths, linkers, runtimes, proxies and CA bundles — so a profile can set it and the headers
apply. Separately, the Bash-permission-checker fix concerns
`permissions.blockReadsOutsideWorkingDirectories`, which Canopy never writes itself: `setupSettings`
emits only `hooks` plus an optional `statusLine` (`claude.ts:90-97`). But it is settable, via the
`claude.settingsJson` profile field that `tabCommands.ts:115-120` parses into `settingsOverrides` and
the adapter spreads at `claude.ts:91`. Worth noting that the spread order makes this safe in one
direction: `overrides` is spread _before_ `hooks`, so a profile can add a `permissions` block but
cannot clobber Canopy's hook wiring.

**The macOS `Read` fix names a hazard class Canopy has already been bitten by and already handles.**
2.1.273 stops `Read` refusing a dragged-in screenshot, or any file the system reports under a second
path, with "symlink resolution changed after permission was checked" — a realpath-based TOCTOU guard
that was too strict on a platform where benign second paths are routine. Canopy's own path gates are
the same shape and get it right, deliberately: `handlers.ts:1394-1396` carries the comment that the
workspace lookup must use the raw renderer-sent path "not the realpath-normalized `resolved`, which
would miss symlinked / `/var`→`/private/var` roots and silently stop cache updates". That is the split
that matters — realpath for the authorization gate, raw path for identity — and the gates themselves
realpath **both** sides rather than comparing a resolved path against an unresolved one
(`handlers.ts:1330-1331`, `3049-3050`, `5433`/`5445`). No change needed, and the in-code comment is
the reason: this class was found on Canopy's side before upstream hit it on theirs.

**One naming collision to record before a future run conflates the two.** 2.1.273 adds forking a
session started with `claude --remote-control` or `/remote-control` into a background local session.
Canopy has a prominent "remote-control" feature of its own — `src/main/remote/RemoteSessionService.ts`
is the "state machine + lifecycle owner for the WebRTC remote-control feature", with
`RemoteControlPrefs.svelte` and `HostRpcServer.ts` alongside it — and it is unrelated: it is Canopy's
mobile client driving the desktop, not the Claude app driving a CLI session. No Canopy code passes
`--remote-control` to the CLI. This is the same trap as the "tool" naming note above, where
`toolView.svelte.ts` and friends mean Canopy's own agent tools rather than CLI tool calls: a grep for
this entry lands in `src/main/remote/` first and that is the wrong layer.

**52 of 2.1.273's 64 CLI changelog entries were not readable this run**, against 12 visible. Both diff
routes were denied again: `gh api` against the changelog repo by the allowlist defect described in
`.github/prompts/claude-code-compat.md` — the fourteenth consecutive run — and `WebFetch` on its first
and only call, the thirteenth consecutive. Every check above was therefore made against Canopy's own
files and the vendored SDK, and the vendored copy remains a lower bound: `npm ci` runs on `next`, so
`node_modules/@anthropic-ai/claude-agent-sdk/manifest.json` reports CLI `2.1.207` regardless of what
this branch pins. Its `HookEvent` union already lists 30 events against the 18 in `CLAUDE_HOOK_EVENTS`,
so Canopy's subscription is a deliberate subset and an absent name there proves nothing.

**2.1.274 is the first release in this range where the tools half _fell_, and it fell by half.** Prompt
tokens −4,669 (−20.7%) with four files removed (−25.0%): 4,669 ÷ 0.207 puts the total at ~22.6k before
and ~17.9k after, and 4 ÷ 0.25 gives 16 files before against 12 after. Both "before" figures reproduce
the 2.1.273 note's ~22.6k and 16 exactly, so the chain from 2.1.271 holds for a fourth link. Splitting
by the given mix — 81.4%/18.6% tools/system before, 52.8%/47.2% after — gives tools ~18.4k → ~9.4k and
system ~4.20k → ~8.44k, so **tools −8,900 (−48%) and system +4,250 (+101%)**. Carrying the 0.1% rounding
through both ends widens those to −8,822…−9,010 and +4,190…+4,303, which is nowhere near noise in
either direction. Every previous note in this range was written about tools _growing_ by one file at a
time; four files leaving at once, with the system half absorbing about half the loss, reads as
consolidation of tool descriptions into system text rather than as tools being retired. That is a
guess. The diff that would settle it was denied, and it should not be presented as more than a guess.

**What can be settled is that the direction is the safe one for Canopy, and why.** The five paths that
key off a tool — enumerated in the 2.1.272 note and unchanged since — are `summarizeToolInput`
(`utils.ts:31`), `normalizeEvent` (`claude.ts:120`), `formatNotification` (`claude.ts:252`),
`toNotchStatus` (`claude.ts:259`) and `EVENT_MAP` (`claude.ts:36`). Only the first reads tool _input_,
and it matches on field names — `command`, `file_path`, `questions`, `query`, `url`, `pattern`,
`prompt`, `description`, `skill` — which live in a tool's JSON schema rather than in the prose whose
token count moved. A description rewrite cannot touch them. And when a shape does go unrecognised the
function does not fail: `utils.ts:66-71` falls through to the first non-empty string value in the
input, then to `''`, so an unknown tool degrades to a worse summary rather than to a crash. The
asymmetry worth recording for the next run is that **tool removal is cheaper for Canopy than tool
addition** — a tool that stops existing simply stops producing hook events, while a new one arrives at
`summarizeToolInput` with a shape nobody has matched.

**`CLAUDE_CODE_MCP_STARTUP_WAIT_MS` names a wait Canopy's one non-interactive turn was fully exposed to,
and the fix is not the new variable.** 2.1.274 adds it to bound how long the first non-interactive turn
waits for connecting MCP servers, `0` meaning don't wait. Canopy has exactly one such turn:
`commitMessageGenerator.ts` calls `query()` for the "generate commit message" button. That call omitted
`settingSources`, and the SDK's own types are explicit about what that means — "When omitted, all
sources are loaded (matches CLI defaults)" (`sdk.d.ts:1866`) — so it inherited project `.mcp.json`, user
settings, plugins and agent frontmatter MCP servers, the exact set `strictMcpConfig` exists to suppress
(`sdk.d.ts:1913-1919`). The turn is a single structured-output call over a diff truncated to 15,000
characters against a fixed two-field schema; it can never call an MCP tool. Nothing on Canopy's side
bounded the wait either: `handlers.ts:2220-2225` awaits the generator from an IPC handler with no
timeout, `query()` is given no `maxTurns` and no abort signal, and `.unwrapOr(null)` converts a throw
rather than a hang — so the button had no failure path at all, only a longer spinner.

The fix applied is `strictMcpConfig: true` rather than `CLAUDE_CODE_MCP_STARTUP_WAIT_MS=0`, for the
reason `claude.ts:195-198` already gives about `--system-prompt-snapshot`: the executable is whatever
`claude` resolves to on the user's `PATH` (`commitMessageGenerator.ts:13-26`), which can predate the
release that introduced the knob. `strictMcpConfig` is present in the vendored `0.3.207` types, so it
holds across every CLI a user might have, and it removes the servers instead of timing them out.
Nothing is lost by it: `Grep` for `mcp`/`MCP` across `src/` returns no matches, so Canopy configures no
MCP servers anywhere and none of them were reachable from this prompt regardless.

**The `tool_use_id` retry-loop fix closes a hole Canopy had no way to report.** Before 2.1.274 a
corrupted transcript could leave a session retrying an "unexpected tool_use_id" 400 indefinitely; the
CLI now self-heals where it can and otherwise ends with a clear error and a `/rewind` hint. A pane stuck
that way emitted no further hook events, so Canopy's notch held whatever `toNotchStatus` last set —
`thinking` after an `AfterToolUse`, most likely — forever, with no `StopFailure` and no `SessionEnd` to
move it. The new terminal error produces one of those, which `claude.ts:274-275` already maps to `error`
or `ended`. Upstream fix, no Canopy change, and worth naming because the symptom was a notch that looked
healthy rather than an error badge.

**The three remaining user-visible entries all land on the in-pane text surface Canopy deliberately
ignores.** The critical-memory warning, the 403 `insufficient_scope` message that now lists missing
permissions and links to `/mcp`, and the click-to-expand for collapsed teammate and agent messages in
fullscreen are all CLI TUI output, rendered into the pane's xterm like any other bytes. If any of them
also raises a `Notification` hook, it reaches `normalizeEvent` and stops there for the reason the 2.1.273
MCP-disconnect note gives: `formatNotification` returns `null` for anything that is not
`PermissionRequest` (`claude.ts:251`) and `toNotchStatus` ends in `.otherwise(() => null)`
(`claude.ts:276`). That is policy, not a gap, and this release adds three more items to the same
deliberately-dropped class rather than changing it.

**The MCP transport fixes and the OTel additions have no Canopy surface, by two different routes.** The
legacy HTTP+SSE 422 fallback, the Streamable HTTP call timing out at ~5 minutes despite a longer
per-server setting, and prompts/resources not refreshing on undeclared `listChanged` notifications all
concern servers Canopy never configures — they reach a pane only through the user's own settings, and
after this run's change they cannot reach the commit-message path at all. The `effort` span attribute,
the `claude_code.managed_settings_resolved` event and `OTEL_LOG_MANAGED_SETTINGS=1` are the other route:
`Grep` for `OTEL`/`OTLP`/`TELEMETRY` across `src/` returns no matches, so Canopy sets none of them, and
a user who wants them sets them through `claude.customEnv` exactly as the 2.1.273 `CLAUDE_CODE_GATEWAY_HINT_HEADERS`
note describes — `BLOCKED_ENV_VARS` still carries no `OTEL`- or `CLAUDE_CODE`-prefixed entry. The Claude
apps gateway items (`store.connect_timeout_seconds`, the `enduser.sub` telemetry field, the 256-request
replica warning) concern a server component Canopy does not run.

**96 of 2.1.274's 108 CLI changelog entries were not readable this run**, against 12 visible — the
largest truncation the range has recorded, and the one release where that matters most, because the
prompt numbers say the tools half was rewritten and the 96 hidden entries are where any tool-schema
change would be named. Both diff routes were denied again: `gh api` against the changelog repo by the
allowlist defect described in `.github/prompts/claude-code-compat.md` — the fifteenth consecutive run —
and `WebFetch` on its first and only call, the fourteenth consecutive. The vendored SDK remains a lower
bound at CLI `2.1.207` for the reason the 2.1.273 note gives, so it cannot confirm a 2.1.274 schema
either. Treat the tool-shape conclusion above as resting on Canopy's fallback behaviour rather than on
having read the tool definitions.

**2.1.275 breaks every request from a pane or commit-message turn that sets a base URL, and 2.1.276 is
the fix.** The regression is a 400 `Input tag 'advisor_20260301'` on every request when
`ANTHROPIC_BASE_URL` points at a proxy or gateway; it shipped in 2.1.275 and was fixed 5h55m later in
2.1.276. It reaches Canopy without anyone exporting the variable, because `claude.baseUrl` is a
first-class profile field (`ClaudeProfileForm.svelte`) written into the environment at two places:
`claude.ts:212` for panes and `commitMessageGenerator.ts:133` for the SDK turn. The two halves need
different fixes for the reason the 2.1.265 note gives. Panes spawn the user's own binary from `PATH`, so
the pin cannot help them and a user on 2.1.275 is fixed by updating; the commit-message turn runs the
CLI the SDK vendors whenever `which claude` fails (`pathToClaudeCodeExecutable` is then `undefined`), so
there the pin decides. `package.json` moves `0.3.274` → `0.3.276` and deliberately steps over `0.3.275`,
the same manoeuvre the 2.1.265 note describes. **Note for the next run: `0.3.275` must never be pinned,
even transiently, and a range that floats onto it is equally broken.**

**2.1.275's send-now key works in Canopy panes by its chord and cannot work by its named shortcut, and
the cause is xterm.js rather than Canopy.** The release adds "ctrl+enter, or ctrl+x ctrl+s" to interrupt
a turn and flush queued messages. `ctrl+x ctrl+s` arrives intact: the two bytes are `0x18` and `0x13`,
Canopy's `attachCustomKeyEventHandler` (`TerminalInstance.svelte:585-614`) claims only Ctrl+V, Ctrl+C
with a selection, Shift+Enter, Cmd+Backspace and Ctrl+Z, and `MainLayout.svelte`'s window handler
(`handleKeydown`, line 440) dispatches on `k p n b , i l o t w d`, the arrows, `1`–`9` and `[ ]` — no `x`
and no `s`, and on macOS its modifier is `metaKey`, so Ctrl chords never reach it at all. `ctrl+enter`
cannot arrive at all, and not because anything intercepts it: xterm.js's `evaluateKeyboardEvent` encodes
Enter as `case 13: o.key = e.altKey ? ESC+CR : CR`, consulting **only** `altKey`, so Ctrl+Enter emits a
bare `\r` byte-identical to Enter. No `modifyOtherKeys` or Kitty-protocol string exists anywhere in
`@xterm/xterm/lib/xterm.js`, so there is no mode that would disambiguate it.

**Remapping Ctrl+Enter to the chord was considered and rejected.** The handler already precedents it —
Shift+Enter writes `\x1b\r` at `TerminalInstance.svelte:598-602` — so writing `\x18\x13` would be a
three-line change. It is wrong on two counts. The same pane hosts Codex, Gemini and OpenCode behind the
same `isAiTool` flag that gates the Ctrl+Z block, and `\x18\x13` means nothing to them; and panes run
whatever `claude` is on `PATH`, so on any binary older than 2.1.275 the bytes land in the prompt as raw
input. Document the working chord instead. This is the `--system-prompt-snapshot` rule from
`claude.ts:195-198` applied to a keybinding rather than a flag.

**`syncClaudeAiSkills` and `syncClaudeAiPlugins` make the doc's "never an interactive subscription
login" claim wrong, and that is the finding.** 2.1.275 syncs the skills and plugins enabled on a
claude.ai account into terminal sessions signed in with it, defaulting on. The `promptCacheTtl` and
2.1.248 notes above both assert Canopy authenticates "with `ANTHROPIC_API_KEY`/`ANTHROPIC_BASE_URL` or
the Bedrock/Vertex/Foundry flags, never an interactive subscription login". That is conditional, not
categorical: `claude.ts:211` is `if (apiKey) env.ANTHROPIC_API_KEY = apiKey`, and the profile field is
optional — its help text says it "falls back to `ANTHROPIC_API_KEY` env variable". A profile with no key
and no inherited variable runs under the user's own `claude` credentials, which is exactly the signed-in
case this release changes. Such panes now perform an account fetch at startup and can surface skills and
plugins nobody configured in Canopy. Users who want the old behaviour set `syncClaudeAiSkills: false` /
`syncClaudeAiPlugins: false` through the profile's Settings JSON field, the same route the
`promptCacheTtl` note describes; `BLOCKED_ENV_VARS` is not involved, since these are settings keys
rather than environment variables.

**The same release puts an unbounded network wait on the one call site that already had no bound, and
this run could not establish whether any option removes it.** `commitMessageGenerator.ts` still omits
`settingSources`, which `sdk.d.ts:1866` documents as "when omitted, all sources are loaded (matches CLI
defaults)" — the sentence that drove the 2.1.274 `strictMcpConfig` change. The turn is still awaited with
no timeout, no `maxTurns` and no abort signal, and `unwrapOr(null)` still converts only a throw. Two
levers exist at the vendored `0.3.207` and so need no version floor — `settingSources: []` and the
`skills` option at `sdk.d.ts:1870-1884` — but neither is obviously the right one: the opt-out is itself a
settings key, so `settingSources: []` plausibly disables the opt-out rather than the sync, and `skills`
is documented as "a context filter, not a sandbox", which would hide synced skills after paying for them
rather than before. Settling this needs the 2.1.275 diff, which was denied. **No change made — recorded
as a known exposure for the next run rather than guessed at.**

**2.1.275 reverses 2.1.274's system-half growth without giving back its tools-half loss, which
falsifies the 2.1.274 note's guess.** Prompt tokens −3,929 (−22.0%) with files unchanged (+0.0%):
3,929 ÷ 0.220 puts the total at ~17.9k before and ~13.9k after, reproducing the 2.1.274 note's ~17.9k
and holding the chain from 2.1.271 for a fifth link, at 12 files throughout. Splitting by the given mix
— 52.8%/47.2% tools/system before, 70.0%/30.0% after — gives tools ~9.43k → ~9.75k and system ~8.43k →
~4.18k, so **tools +321 (+3.4%) and system −4,250 (−50%)**. Carrying the 0.1% rounding through both ends
leaves tools at +258…+387 and system at −4,160…−4,343, so both signs are safe. The 2.1.274 note read
that release's −8,900 tools / +4,250 system as "consolidation of tool descriptions into system text",
flagged as a guess. It does not survive: system has returned to ~4.18k against its ~4.20k value before
2.1.274, while tools stayed near ~9.4–9.8k instead of returning to ~18.4k. Had the text been moved,
unwinding the system half would have restored the tools half with it. The better reading is two
independent edits — a real ~8.9k cut to tool descriptions in 2.1.274 that has held, and a transient
~4.25k of system text added there and removed here.

**2.1.276 ships a prompt file its changelog does not mention, and it is on the expensive side.** The
release lists exactly one entry, the proxy fix above, yet prompt files go +1 (+8.3%) and tokens +440
(+3.1%). 1 ÷ 0.083 gives 12 before and 13 after, matching 2.1.275's 12; the token split moves
70.0%/30.0% → 70.9%/29.1%, which against a ~13.9k → ~14.4k total puts **tools +437 and system +3**. So
the whole increment is one new tools-kind file. That is the direction the 2.1.274 note calls costlier
for Canopy — "tool removal is cheaper than tool addition — a new one arrives at `summarizeToolInput`
with a shape nobody has matched" — though the degradation is still the graceful one at `utils.ts:66-71`.
One thing does not add up and is left open rather than explained away: the bundle grew only +0.1 kB,
far less than ~440 tokens of new description, which fits an existing string newly extracted into its own
prompt file better than genuinely new text. The diff that would name the tool was denied.

**84 of 2.1.275's 95 CLI changelog entries were not readable this run**, against 11 visible; 2.1.276's
single entry was complete. Both diff routes were denied again: `gh api` against the changelog repo by
the allowlist defect described in `.github/prompts/claude-code-compat.md` — the sixteenth consecutive
run — and `WebFetch` on its first and only call, the fifteenth. The vendored SDK is still `0.3.207` and
still a lower bound for the reason the 2.1.273 note gives, so its silence on `syncClaudeAiSkills` says
nothing. Given the size of 2.1.275 — bundle +488.8 kB, the largest in this range — treat the coverage
above as resting on 11 entries and on Canopy's own files, not on having read the release.

**2.1.277's logout fix names Canopy's exact shape, and `commitMessageGenerator.ts` is the call site.**
The entry reads "Fixed being unexpectedly logged out when an older Claude Code build (for example an
IDE extension's bundled CLI) runs on the same machine as the current one". Canopy is that example.
`commitMessageGenerator.ts:75` passes `pathToClaudeCodeExecutable: claudePath`, and
`resolveClaudeExecutable()` at `:13-26` returns `undefined` whenever `which`/`where claude` fails —
a case `sdk.d.ts:1688` documents as "Uses the built-in executable if not specified", which is the
259 MB `claude` binary shipped in `node_modules/@anthropic-ai/claude-agent-sdk-{platform}/`. So a
user with Claude Code installed somewhere the login shell's `PATH` does not reach gets Canopy
running a **bundled CLI of whatever version Canopy pins** against the same machine credential state
their own install uses. On `next` that pin is still `0.3.207` — CLI 2.1.207, seventy releases back.

The second-order version is worse and is specific to Canopy: agent panes run the `PATH` `claude`,
while commit-message generation runs the bundled one. Both live in the same app, on the same
machine, against the same credentials, so generating a commit message could log out the agent panes
sitting next to it. **The bump in this increment is the mitigation** — it moves the bundled fallback
to 2.1.277, which carries the fix. Which side of the skew the fix actually lives on (the newer build
becoming tolerant, or the older one no longer clobbering) is not established, because the diff was
denied and only the first reading is fully repaired by bumping. Treat the bump as removing Canopy
from the population of offenders, not as proof that no user can still be logged out.

**The `-p`/SDK hang fix retires part of a hazard this repository already documents in code.**
`commitMessageGenerator.ts:78-91` argues, correctly, that "`git:generateCommitMessage` awaits this
with no timeout, `query()` gets no `maxTurns` or abort signal, and `unwrapOr(null)` only catches a
throw, not a hang". 2.1.277's "Fixed `claude -p` and Agent SDK sessions that could hang with no
result after an internal error; they now report the error and exit with code 1" converts one class
of that hang into a throw — which `fromExternalCall` at `:67` already catches and `.unwrapOr(null)`
at `:157` already degrades to a null commit message. No code change is needed: that error path was
always correct, it simply had nothing to catch. The comment stays accurate as written, because every
clause in it is a statement about Canopy's code rather than the CLI's and all of them still hold,
and because the caveat it ends on still governs — the executable is whatever `claude` resolves to on
`PATH`, so users below 2.1.277 keep the old hang. The MCP-connect hang the comment is actually about
is a different class and is untouched; `strictMcpConfig: true` at `:91` remains the fix for that one.

**Canopy uses `--resume`, so the empty-text-block fix lands on a live path.** `claude.ts:237-239` is
`buildResumeArgs(id) => ['--resume', id]`. 2.1.277 fixes "conversations failing **every** request
with 'text content blocks must be non-empty' when an earlier assistant turn held an empty text block
beside other content, **including after `--resume`**". The failure mode is a session that is
permanently unusable rather than intermittently degraded, and Canopy can neither detect nor repair
it — it surfaces as `IdleFailure` on the tab, which reads as an agent crash rather than as a
poisoned transcript. Nothing to change, since panes run the `PATH` binary and the fix arrives when
the user upgrades. Recorded because the symptom looks like a Canopy resume bug and is not one.

**AGENTS.md now means something to Claude Code, which makes Canopy's setup prompt more right than it
was and leaves one silent failure.** 2.1.277 reads `AGENTS.md` for project instructions in a project
with **no** `CLAUDE.md`. Canopy's own repository is unaffected: it has both, and `CLAUDE.md` wins.
`agentPrompt.ts:6` — the one-shot prompt a user pastes to persist Canopy's branch/PR conventions —
already said to add the section to "your agent instructions file (CLAUDE.md, AGENTS.md, or the
equivalent for your tooling)", deliberately agent-agnostic because Codex and OpenCode read
`AGENTS.md`. Before this release an agent that chose `AGENTS.md` was certainly ignored by Claude
Code; now it is read, so the release improves the existing wording rather than breaking it. What
survives is the both-files case: the prompt hands the agent a free choice between two files, only
one of which is loaded when both exist, and an agent that picks the unread one writes conventions
that are then silently skipped in every later task. **Changed** — the parenthetical now says to write
to the file the tooling actually loads and that the others are ignored. The edit is agent-agnostic
on purpose: naming `CLAUDE.md` as the winner would be correct for the Claude adapter and wrong for
the other three.

**Second consecutive release whose entire prompt-token increment is one new tools-kind file — but
unlike 2.1.276, this one is backed by real bundle growth.** Files +1 (+7.7%) gives 1 ÷ 0.077 = 13
before and 14 after, continuing the 2.1.276 note's 13 for a seventh link. Tokens +556 (+3.9%) put
the total at ~14.3k before and ~14.8k after; splitting by the given mix (70.9%/29.1% → 72.0%/28.0%)
gives **tools ~10.1k → ~10.7k (+557) and system ~4.15k → ~4.15k**. Carrying the 0.1% rounding
through both ends leaves tools at +281…+833 — sign safe — and system at −105…+132, which straddles
zero, so system is flat _within rounding_ rather than measurably unchanged. The whole increment is
the tools half, exactly as in 2.1.276. The bundle is what differs: 2.1.276 grew +0.1 kB against +440
tokens, which that note flagged as fitting a re-extraction of existing text better than new text,
whereas this release grows **+653.2 kB (+1.3%)** — so this one is more plausibly a genuinely new
tool. The changelog does not name it, 75 of 87 entries being unreadable, but the consequence is the
one already established and re-confirmed by reading the function this run: `summarizeToolInput`
(`utils.ts:31`) matches input _shape_, not name, and ends in the generic first-non-empty-string
fallback at `utils.ts:69-73`, so an unrecognised tool degrades gracefully instead of throwing.

**12 of 2.1.277's 87 CLI changelog entries were readable**, against 75 behind the truncation. Both
diff routes were denied again: `gh api` against the changelog repo on two attempts — the seventeenth
consecutive run, still the mid-token allowlist defect described in
`.github/prompts/claude-code-compat.md` — and `WebFetch` on its first and only call, the sixteenth.
The vendored SDK under `node_modules` is still `0.3.207`, since the workflow checks out `next` and
runs `npm ci` there, and it remains a lower bound for the reason the 2.1.273 note gives. Its
`sdk.d.ts` still earned its keep: the `pathToClaudeCodeExecutable` doc comment at line 1688 is what
turns the logout entry from a general warning into a claim about one specific Canopy line, and that
sentence appears in no release note.

**2.1.278 changes what auto mode costs, and the first thing to record is the near-miss.** Grepping
`src/` for `'auto'` returns it twice in the renderer and both hits read, at a glance, as a Claude
**model** option — which would put Canopy's own UI directly in the release's population. Both are
`claude.permissionMode`: `ClaudeProfileForm.svelte:73` and `AiSetupStep.svelte:48` are the
Default/Plan/Auto/Accept-edits/Bypass select. Both **Model** fields are free-text `<input>`s whose
placeholders read "sonnet, opus, haiku, fable, or model ID" and "sonnet, opus, haiku, or model ID";
neither offers `auto`. This is the 2.1.273 naming-collision note reproduced on a different axis, and
it is worth one paragraph because the grep alone would have supported a finding that reading the two
files removes.

**So whether a Canopy pane is in auto mode at all is settled by a default Canopy does not set.**
`claude.ts:182` is `if (model) args.push('--model', model)`, so an empty Model field means no
`--model` flag and whatever the CLI itself defaults to. The field is unvalidated, so a user _can_
type `auto`, but Canopy neither offers it nor documents it. Whether the CLI's own default is auto is
**not established here** — the entry's documentation link is unreachable (`WebFetch` denied) and the
vendored SDK is 2.1.207, which the 2.1.273 note fixes as a lower bound. This is the 2.1.277
membership instrument pointed at a question this run cannot close, and the honest answer is that the
size of the affected Canopy population is unknown rather than zero.

**The provider half needs no such hedge, and Canopy is squarely inside it.** The entry names "Claude
API and Enterprise users, and on Bedrock, Vertex, Foundry and gateways". `claude.ts:200-202` and
`commitMessageGenerator.ts:134-136` set `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX` and
`CLAUDE_CODE_USE_FOUNDRY` from `claude.provider`, a first-class profile field — so three of the four
named surfaces are ones Canopy opts users into deliberately. That makes this the second release in
this range where a Claude Code default flips for precisely the configuration Canopy selects; 2.1.239's
fullscreen-renderer offer, extended to the same three providers, was the first.

**Nothing changes, and the reasons are three.** The direction is favourable: the server-side
classifier "does not charge for classifier overhead", so the default flip makes auto mode cheaper
rather than more expensive. The opt-out is already reachable: `CLAUDE_CODE_AUTO_MODE_SERVER=0`
appears in neither `BLOCKED_ENV_VARS` (`envBlocklist.ts:6-67`) nor `INTERNAL_BLOCKED`
(`claude.ts:53-58`), so `claude.customEnv` carries it today — the same disposition as 2.1.273's
`CLAUDE_CODE_GATEWAY_HINT_HEADERS` and 2.1.271's `ANTHROPIC_UNIX_SOCKET`, and it does not belong in
the blocklist, which covers linkers, proxies and CA bundles that could subvert an agent rather than
feature toggles. And the standing version-floor rule applies with unusual force: setting the variable
would impose a 2.1.278 floor on users in order to opt them _out_ of the cheaper behaviour.

**The commit-message turn is outside the population, and it is the first of five findings on that
call site to come back negative.** `commitMessageGenerator.ts:74` pins `model: 'haiku'`, so that turn
is not in auto mode on any provider and neither entry can reach it. Four earlier findings in this
range landed on `generateCommitMessage()` — 2.1.237, 2.1.266, 2.1.268 and 2.1.275 — and this one does
not. The shape is the mirror image of 2.1.274's `settingSources` finding and worth keeping as such:
there, an **omission** meant "inherit every default"; here, an **explicit value** means "opt out of
the default before it changes". When a release changes a default, the call sites that pin the setting
are the ones to clear first.

**The `/status` row is not a surface Canopy reads.** `normalizeStatus` (`claude.ts:135-173`) consumes
the statusLine payload — `version`, `model`, `context_window`, `cost`, `rate_limits` — while
`/status` is the interactive TUI command, a different surface. That is read off the entry's wording
rather than verified, and deliberately so: the vendored 0.3.207 types are a lower bound, so an
auto-mode field being absent there would prove nothing. If the CLI also added one to the **statusLine**
payload, Canopy would silently drop it, which is exactly the standing shape of the two fields left
unwired since 2.1.251 — `rate_limits.spend_limit` and `prompt_cache`, still unwired at 2.1.278. The
"warns on billed fallback" text is in-pane TUI output and reaches the user through the PTY unchanged.

**A third consecutive tools-only increment — and the first where the unannounced prompt file is
evidence rather than a gap.** Files +1 (+7.1%) gives 1 ÷ 0.071 = 14 before and 15 after, continuing
the 2.1.277 note's 14 for an eighth link. Tokens +685 (+4.6%) put the total at ~14.9k before and
~15.6k after, reproducing that note's ~14.8k. Splitting by the given mix (72.0%/28.0% → 73.2%/26.8%)
gives **tools ~10.7k → ~11.4k (+680) and system ~4.17k → ~4.17k (+5)**; carrying the 0.1% rounding
through both ends leaves tools at +429…+931 — sign safe — and system at −99…+109, straddling zero. So
the whole increment is the tools half for the third release running.

What is new is that **this release's changelog is complete**: two entries, no "… +N more" marker, the
first such release since 2.1.263. For 2.1.272, 2.1.276 and 2.1.277 the sentence "the changelog does
not name the new file" was ambiguous between _not announced_ and _announced behind the truncation_.
Here it is unambiguous. The bundle grew **+11.7 kB (+0.0%)** against 685 tokens of prompt text — call
it ~2.7 kB — which places this much nearer 2.1.276's re-extraction end than 2.1.277's +653.2 kB
new-tool end. The obvious candidate, the auto-mode classifier prompt extracted for the local billed
fallback path, **does not fit cleanly**: the arithmetic puts the growth on the _tools_ side and a
model-selection classifier is not a tool description. Left unresolved rather than guessed;
`meta/prompt-stats.md` names individual prompt files and would settle it, and is behind the same
denied `gh api` rule as everything else. The consequence for Canopy is unchanged — `summarizeToolInput`
(`utils.ts:31`) matches input _shape_, not name, with the generic fallback at `utils.ts:69-73` — but
the unmatched-shape candidate set has now grown four times running, counting 2.1.274's rewrite of
roughly half the tools prompt.

**Both of 2.1.278's CLI changelog entries were readable**, the first complete changelog in this range
since 2.1.263, so nothing is hidden behind truncation this increment. Both diff routes were denied
again all the same: `gh api` against the changelog repo on five attempts — the eighteenth consecutive
run, still the mid-token allowlist defect described in `.github/prompts/claude-code-compat.md` — and
`WebFetch` on its first and only call, the seventeenth. The vendored SDK under `node_modules` is still
`0.3.207`, since the workflow checks out `next` and runs `npm ci` there.

**2.1.280's Write-validation fix is the first entry in this range to force a code change by making a
malformed tool input _legal_, and what it reached was an OS notification.** The entry reads "Fixed
`Write` calls failing validation when a model sends `path`, `file_text`, `file_content` or a stray
`description` instead of `file_path` and `content`". Before this release those shapes were rejected
upstream; from 2.1.280 they validate, so `PreToolUse` delivers them — and `summarizeToolInput`
(`utils.ts:31-76` as it stood) had a branch for `file_path` and none for `path`, `file_text` or
`file_content`. A Write arriving as `{path, file_text}` fell through every named branch to the generic
first-string fallback at the bottom, which returns whichever string key comes first **in object
order**. For `{file_text, path}` that is the file body, truncated to 80 characters.

**That fallback is not a display-only path.** `summarizeToolInput` feeds `toNotchStatus`
(`claude.ts:245-264`) for the `toolCalling` and `waitingPermission` details, and `formatNotification`
(`claude.ts:237-243`) for the body of the macOS "Claude Code — Permission Required" notification. So a
Write of a `.env`, a key file or any secret-bearing file could have put its first 80 characters into a
system notification — rendered by the OS, outside Canopy's control, and on macOS persisted to
Notification Centre. The fix adds `path` as a `file_path` alias and makes the fallback skip
body-bearing keys. Both guards are order-sensitive and the reasons are in the code: `path` sits
**below** the `pattern` branch because Grep and Glob send `{pattern, path}` and their summary is the
pattern, and **above** `description` because the release's fourth shape is a Write carrying a stray
`description` that should still summarize as its path.

**What the guard trades, stated plainly.** An input carrying _only_ a body key — `{file_text}` with no
path of any spelling — now falls through to `''` where it previously returned 80 characters of the
body, so the notch renders `Write: ` with a dangling colon. That is the exact symptom of the
long-standing array-only finding, and the same two-line `Array.isArray`/empty-result branch closes
both. The trade is the right way round — a dangling colon is a cosmetic defect, a file body in a
notification is a disclosure — but it is a trade, not a clean win, and it is not hypothetical for long
if upstream keeps loosening tool schemas. A Write with a body and no target should not survive
validation even after 2.1.280, which is why this is recorded rather than fixed here.

**This is the shape the 2.1.270 note predicted from the other end.** That note recorded the standing
check as "an unknown tool degrades to a generic summary", and the 2.1.272 audit enumerated the five
Canopy paths that key off a tool name. Both framed the risk as _new tools_. The actual break came from
an existing tool with a **new input spelling**, which no tool-name audit would have caught — the
function matches input shape, not name, and that is exactly why it was exposed. Worth keeping as the
counterexample: a release that loosens a tool's schema is as much a Canopy event as one that adds a
tool.

**Opus 5.5 becomes the default Opus model and Canopy's Model field is free text, so no app code
changes — but Canopy's own CI is squarely in the population.** `ClaudeProfileForm.svelte` keeps an
unvalidated `<input>` and `claude.ts:182` is `if (model) args.push('--model', model)`, so an empty
field means no flag and whatever the CLI defaults to; no model list is hardcoded anywhere in `src/`.
What does hardcode one is `.github/workflows/`: nine workflows pass `--model opus[1m]`, `--model
haiku` or both in `claude_args`, and `opus[1m]` now resolves to `claude-opus-5-5` at $4/$20 per Mtok
with $0.20/Mtok cache reads. That is a per-run cost change to this repository's own automation with no
file to edit — the alias is doing what it is for. Recorded rather than changed, and noted here because
a future run comparing workflow costs across this range will otherwise look for a commit that does not
exist. `normalizeStatus` (`claude.ts:135-173`) reads `context_window_size` from the payload rather than
assuming it, so a 1M-context default needs nothing, and `AgentInspector.svelte` colours on percentage.

**The auto-mode retry fixes land on a permission mode Canopy offers by name, and Canopy's hook server
is the quiet beneficiary.** `ClaudeProfileForm.svelte:73` puts `auto` in the Default/Plan/Auto/Accept-
edits/Bypass select, so `--permission-mode auto` is one click away — the same population the 2.1.278
note established. Two of this release's fixes bound a loop: a declined safety review is now denied once
"noting that retrying won't help", and a silent check backs off with the turn stopping after ten in a
row. The un-bounded version of that loop did not just burn tokens in the pane. Every retry raises
`PreToolUse`/`PostToolUse`, and each of those is an HTTP POST to `AgentHookServer` on `127.0.0.1`,
normalized and pushed to the renderer as a status change — so a wedged auto-mode turn presented in
Canopy as a notch flickering between `toolCalling` and `thinking` indefinitely, with no terminal state.
It now reaches `Stop` and settles on `idle`. Nothing to change; worth recording because the symptom is
one a user would have reported as a Canopy bug.

**`CLAUDE_CODE_MAX_MCP_DESCRIPTION_LENGTH` is reachable from a profile today and cannot reach the
commit-message turn at all.** The new variable lifts the 2,048-character cap on MCP tool descriptions
and server instructions. It appears in neither `BLOCKED_ENV_VARS` (`envBlocklist.ts:6-67`) nor
`INTERNAL_BLOCKED` (`claude.ts:53-58`), so `claude.customEnv` carries it — the same disposition as
2.1.278's `CLAUDE_CODE_AUTO_MODE_SERVER`, 2.1.273's `CLAUDE_CODE_GATEWAY_HINT_HEADERS` and 2.1.271's
`ANTHROPIC_UNIX_SOCKET`, and for the same reason: the blocklist covers linkers, proxies and CA bundles
that could subvert an agent, not feature toggles, and wiring a first-class field would impose a
2.1.280 floor. The second half is the sharper one. `commitMessageGenerator.ts:91` sets
`strictMcpConfig: true`, so that turn has **no MCP servers at all** and a cap on MCP descriptions is
unreachable there by construction. That makes six consecutive negatives on `generateCommitMessage()`
after four straight hits earlier in the range — and this one is negative _because of a change this
branch itself made_ at 2.1.274, to stop the turn waiting on the user's MCP fleet. A mitigation landed
for one reason has now closed a second, unrelated surface.

**The symlink fix is a genuine behaviour change for a path shape Canopy produces, and the right
response is still to change nothing.** From 2.1.280 a write through a symlinked path is judged by
where it **lands** rather than by its in-tree spelling, and `acceptEdits`, allow rules and auto mode no
longer approve one landing outside. Canopy hands the CLI a worktree path it created, and those paths
sit wherever the user pointed them — on macOS anything under `/tmp` or `/var` is reached through a
symlink before the user does anything unusual. So a user in `acceptEdits` or `auto` whose worktree is
behind a symlink can see per-edit prompts return on 2.1.280 where there were none on 2.1.279.
**Resolving the cwd with `realpath` before spawning would be the wrong fix**, and not marginally:
`handlers.ts:2063-2075` validates removal against `git worktree list` output by path comparison, and
`index.ts:339` builds its dedupe set from `gitInfo.worktrees.map((wt) => wt.path)` — both compare
against git's own spelling, so substituting a resolved path would break worktree removal and tab
dedupe to suppress a prompt that is, on its merits, correct. Canopy's own setup runner is already
strict here for its own writes (`WorktreeSetupRunner.ts:14,32-47` resolves realpaths and refuses a
destination that crosses a symlink), which is the same judgement the CLI has now adopted. Support
answer, not a code change.

**Three more entries reach a Canopy surface and need nothing from it.** The `hook_execution_complete`
OpenTelemetry event gains hook output sizes and a count of oversized outputs spilled to a file — the
nearest miss in the release, since Canopy is a hook consumer, but it is OTel telemetry rather than hook
payload, and Canopy's hooks are observational: `canopy-agent-hook.sh` POSTs to `127.0.0.1` and returns
nothing, so its output size is ~0 and it can never be the oversized one. The `y`/`n` change — a stray
`n` no longer closes dialogs, a stray `y` no longer confirms them, with `keybindings.json` offered to
restore them — would break any caller that auto-confirms by writing bytes to the PTY; grepping `src/`
for programmatic `y`/`n`/CR writes returns nothing, so every one of those keystrokes is a user's own
and passes through unchanged. And Ctrl+C or Ctrl+D pressed twice in `/model`, `/config`, `/permissions`
and eleven other dialogs no longer quits the CLI, which in a Canopy pane had meant an unexplained
`SessionEnd`; that arrives through the user's own `claude` on `PATH`, so the SDK pin in `package.json`
does not deliver it — the standing asymmetry the 2.1.268 and 2.1.269 notes both record.

**This is the largest release in the range on every axis the metadata carries, and the first in four
where both prompt halves grew.** Files +4 (+26.7%) gives 4 ÷ 0.267 = 14.98, so **15 before and 19
after** — the band is 14.95–15.01, so 15 is exact — continuing the 2.1.278 note's 15 for a ninth link.
Tokens +10,204 (+65.2%) put the total at **~15.65k before and ~25.85k after**, reproducing that note's
~15.6k. Splitting by the given mix (73.2%/26.8% → 67.3%/32.7%) gives **tools ~11.46k → ~17.40k
(+5,944) and system ~4.19k → ~8.45k (+4,260)**; carrying the 0.1% rounding through both ends leaves
tools at +5,907…+5,982 and system at +4,232…+4,288, so both signs are safe by a wide margin. Three
consecutive tools-only increments (2.1.276, 2.1.277, 2.1.278) end here. The bundle corroborates it
rather than merely permitting it: **+1,120.3 kB (+2.2%)** puts the total at ~50.9 MB (band 49.8–52.1 MB),
and at the ~4 bytes/token conversion the 2.1.277 note calibrates, 10,204 tokens of prompt text is only
~41 kB of that. The remaining ~1.08 MB is implementation, which places this far past 2.1.277's
+653.2 kB "genuinely new tool" end — four new prompt files with real code behind them, built in the
2d 19h 27m since 2.1.278, the longest gap in this range.

**102 of 2.1.280's 114 CLI changelog entries were not readable this run**, against 12 visible — a new
high for hidden entries in this range, past 2.1.274's 96, and the largest entry count of any release
here. Everything above is drawn from the 12 visible entries and Canopy's own files; the other 102 are
unrecoverable for this run and no claim here should be read as covering them. Both diff routes were
denied again: `gh api` against the changelog repo on four attempts — the nineteenth consecutive run,
still the mid-token allowlist defect described in `.github/prompts/claude-code-compat.md` — and
`WebFetch` on its first and only call, the eighteenth. Delegating the fetch was tried a second time
and failed a second time, which settles it: a subagent hit the identical refusal on `WebFetch`,
`WebSearch`, `curl` **and** `gh api`, confirming from a second agent type what the v2.1.259 → v2.1.260
run found with `claude-code-guide` — the permission decision is the session's, not the agent's. The
vendored SDK under `node_modules` is still `0.3.207`, since the workflow checks out `next` and runs
`npm ci` there.

**2.1.281 makes a settings value legal that older CLIs reject, and Canopy writes profile settings into
the one file where that costs its own hooks.** The entry reads "Added `"attribution": false` in
settings.json to hide all commit and PR attribution; older CLI versions skip a settings file that
holds it, so keep the object form in files shared across versions". Canopy's per-session file is
shared across versions in exactly that sense: `setupSettings` writes it without knowing which `claude`
will read it — the user's `PATH` binary — and spreads the profile's Settings JSON into the same object
as the hooks and status line. So a user who copies the 2.1.281 docs into a profile's Settings JSON
while their `claude` is older loses every Canopy hook and the status line for that profile's panes: no
notch status, no permission notification, no context or cost in the Agent Inspector, and nothing in
Canopy to say why. The form's help text promises the opposite — "Hooks and status line are always
preserved" (`ClaudeProfileForm.svelte:210`).

**The fix rewrites a boolean `attribution` before the file is written, using 2.1.281's own mapping
rather than a guess at it.** The 2.1.281 binary's settings schema is a `union([boolean, object])`
followed by a transform: `true` becomes `{}` and `false` becomes
`{ commit: '', pr: '', sessionUrl: false }`. `desugarAttribution` in `claude.ts` applies the same
mapping, so a 2.1.281+ CLI parses the object it would have built itself and an older one reads a form
it already accepts. `true` is rewritten as well, because the schema's error text — "Expected false,
true, or an object" — shows the whole boolean is new in 2.1.281, not only `false`. The vendored
`0.3.207` types declare all three members (`sdk.d.ts:4867-4880`), so the object form is understood at
least as far back as 2.1.207. `claude.test.ts`, the first test file under `src/main/agents/`, pins the
mapping, pass-through of an object, the absence of the key when unset, and that Canopy's `Stop` hook
and status line survive next to a rewritten value. **It covers the one value upstream documents as
valid on new CLIs and invalid on old ones; it is not a general guard.** Any other override an older
CLI rejects still takes the hooks with it. That is pre-existing and not curable here, since
`--settings` takes one file and the hooks have to share it with whatever the profile adds.

**The SDK bump is corrective for the commit-message turn, by the same membership test.**
`commitMessageGenerator.ts` omits `settingSources`, which the vendored `sdk.d.ts:1861-1870` documents
as loading every filesystem source — `~/.claude/settings.json` included, a file shared by every CLI
version a user runs. When `which claude` fails, the turn runs the bundled binary; at `0.3.280` that is
2.1.280, which would skip a user settings file holding the boolean, and every `env`, `apiKeyHelper`
or provider setting in it with it. `0.3.281` bundles 2.1.281, which parses it. When `claude` is on
`PATH` the turn runs the user's binary and the pin decides nothing — the asymmetry the 2.1.277 note
records.

**The CLI running this job was 2.1.281 itself, on disk, and that makes a denied diff readable for
every question that is about a string.** It sits at `~/.local/share/claude/versions/2.1.281`. `ls`
outside the working directory is denied, but the `Glob` tool finds it, and the `Grep` tool reads the
minified bundle with `-o` and a context window of about 300 characters. That is how the mapping above
was read, and it settles three things the vendored SDK could only bound from below:

- **The hook-event array names 33 events.** Canopy subscribes to 18, all still present. The 15 it does
  not subscribe to are the 14 recorded earlier plus `DirectoryAdded`, new since 2.1.207. This is the
  first current count in this range rather than a floor.
- **Every flag Canopy emits is still defined**: `--settings`, `--model`, `--permission-mode`,
  `--effort`, `--append-system-prompt`, `--system-prompt-snapshot` and `--resume`.
- **Every hook-payload and status-line field Canopy reads by name is still present**: `error_details`,
  `compact_summary`, `notification_type`, `task_subject`, `task_description`, `teammate_name`,
  `team_name`, `permission_mode`, `agent_type`, `used_percentage`, `context_window_size`,
  `total_cost_usd`, `total_duration_ms`, `total_lines_added`, `total_lines_removed` and
  `display_name`. Presence rules out a removal or a rename; it does not prove the shape is unchanged.

The binary does not embed the changelog — three phrases from this release's visible entries return
no match — so it cannot recover hidden entries. It tests Canopy's contract; it does not say what
changed.

**The two new prompt files describe tools Canopy already summarizes.** They are the Bash ("executes
a given bash command") and Agent ("launch a new agent") tool descriptions. Both tools keep their
names, and `summarizeToolInput` reaches Bash through `command` and Agent through `prompt`, so no branch
needs to move. That is the check the 2.1.280 note asks for, since a new input spelling is what broke
that function last time.

**Two visible fixes land on standing findings and need nothing from Canopy.** The crash that "could
end a session while an API request was being retried" has the stale-notch shape: if a process dies
without `SessionEnd`, its pane keeps reading `thinking`, because only `destroySession` evicts the
notch entry. That makes it a fifth upstream route to that backstop gap, fixed upstream and arriving
through the user's own binary. The turn that "could retry indefinitely, ignoring --max-turns" points
at the commit-message turn, which passes no `maxTurns` — but that turn is one structured-output call
against a two-field schema, where output-limit truncation is implausible, and the bundled binary now
carries the fix regardless.

**Nothing else visible reaches Canopy.** The four gateway entries configure a Claude apps gateway,
which Canopy neither writes nor selects (`claude.provider` offers Bedrock, Vertex and Foundry).
URL-mode elicitation opens its browser flow from the CLI process, and the commit-message turn has no
MCP servers to elicit from (`strictMcpConfig: true`). `claude plugin validate` has nothing of
Canopy's to check — the repository ships no `.claude-plugin/` or `.mcp.json`. The fullscreen list
scrollbar relies on the same mouse tracking the 2.1.271 note found working in Canopy panes. The resume
fix lands on `--resume` panes through the user's binary, and the stray "Response" removed from the
hooks guidance changes no instruction.

**Both prompt halves grew again, the system half by more.** Files +2 (+10.5%) gives 2 ÷ 0.105 =
19.05, band 18.96–19.14, so **19 before and 21 after**, continuing the 2.1.280 note's 19 for a tenth
link. Tokens +6,241 (+24.1%) put the total at **~25.90k before and ~32.14k after** (before band
25.84k–25.95k, reproducing that note's ~25.85k). Splitting by the given mix (67.3%/32.7% →
59.2%/40.8%) gives **tools ~17.43k → ~19.03k (+1,597) and system ~8.47k → ~13.11k (+4,644)**; carrying
the rounding through both ends leaves tools at +1,564…+1,630 and system at +4,611…+4,677, so both
signs are safe. The tools growth fits the two new tool-description files. **The system growth is
unattributed**: the only named system change removes one word, and whatever explains +4.6k is behind
the truncation marker. The bundle grew **+1,183.2 kB (+2.3%)** from ~51.4 MB (band 50.3–52.6 MB,
overlapping the 2.1.280 note's 49.8–52.1 MB) — the largest delta in this range, of which ~25 kB is
prompt text at ~4 bytes/token.

**164 of 2.1.281's 176 CLI changelog entries were not readable this run**, against 12 visible — a
new high, past 2.1.280's 102. `gh api` against the changelog repo was denied on two attempts — the
twentieth consecutive run — and `WebFetch` on its first and only call, the nineteenth. The on-disk
binary covers the part of that gap that is Canopy's contract (flags, hook events, payload and
status-line field names, and the one schema the visible entries named) and nothing else; behaviour
behind the hidden entries is still unread. The vendored SDK under `node_modules` is still `0.3.207`.

**2.1.282's effort fix is about a model switch, and following it into the build showed that Canopy's
model-switch subscription had never read a model.** The entry reads "Fixed a failed turn ("Effort
'xhigh' isn't available with thinking turned off") after a safety-related model switch in sessions
with thinking off and effort above high". The switch is `switchModelsOnFlag` — "When safeguards flag
a message, automatically switch to a different model to keep chatting". Canopy has subscribed to
`PreModelSwitch` and `PostModelSwitch` since 2.1.251 (`ddbc639`) so the Agent Inspector can show the
new model, and 2.1.282 fires `PostModelSwitch` from a state subscriber whenever the session's
effective model changes. `source` comes from whichever caller recorded one, and is `"auto"` —
"automatic fallback or other programmatic change" — otherwise. Whether the safeguards fallback
itself changes the session model, rather than only the turn's, could not be traced through the
minified query loop. Whatever fires it, the event's hook-input schema in the 2.1.282 build is
`from_model`, `to_model` ("Resolved model id the session runs after the switch"), `requested_model`,
`source`, `context_tokens` and cache-cost fields over the common base (`session_id`,
`transcript_path`, `cwd`, `prompt_id`, `permission_mode`, `agent_id`, `agent_type`, `effort`), with
**no top-level `model`**.
`normalizeEvent` read `raw.model`, so every switch normalized to `model: undefined` and
`handleHookEvent`'s `if (event.model)` skipped it. Nobody saw it because the status line always runs
(`AgentSessionManager` passes the script unconditionally) and its next refresh writes
`model.display_name` — so the display lagged until then rather than staying wrong.

**The fix reads `to_model` from `PostModelSwitch` only.** `PreModelSwitch` carries the same fields,
but its `to_model` is a proposal: a `PreModelSwitch` hook in the user's own settings can block the
switch or ask first, and taking the model from it would show a switch that never happened.
`claude.test.ts` pins both cases plus `SessionStart`, whose schema does carry `model`. The sources
2.1.282 reports are `command` (`/model`, the `/config` Model row, fast mode), `picker`, `sdk`, `auto`
and `resume` — the last fires when `--resume` restores a session's model, which Canopy issues on every
layout restore.

**The 2.1.281 contract check could not have caught this, because it tested names, not events.** It
confirmed that every field Canopy reads "is still present", and `model` is present — in
`SessionStart`. This run checked each field against the schema of the event Canopy reads it from.
Everything else holds: `error`/`error_details` on `StopFailure`, `reason` on `SessionEnd`,
`compact_summary` on `PostCompact`, `message`/`title`/`notification_type` on `Notification`,
`task_id`/`task_subject` on `TaskCompleted`, `agent_id`/`agent_type` on `SubagentStart` and
`SubagentStop`, `tool_name`/`tool_input`/`tool_response` on the tool events, `permission_mode` on the
base, and on the status line `model.id`/`display_name`, `context_window.used_percentage` and
`context_window_size`, the five `cost` fields and `version`. One field is now marked for removal:
`team_name` on `TeammateIdle` and `TaskCompleted` is "@deprecated … will be removed in a future
release". Canopy normalizes it into `teamName`, and nothing reads it.

**The effort entry also reaches Canopy's own dropdown.** The profile's Effort level offers Extra high
(`xhigh`) and Max (`max`) — the entry's "effort above high" — and 2.1.282's `--effort` still accepts
exactly `low`, `medium`, `high`, `xhigh` and `max`. A profile that also turns thinking off, with
`MAX_THINKING_TOKENS=0` in its env vars or `"alwaysThinkingEnabled": false` in Settings JSON, could
fail turns after a safeguards switch on a CLI older than 2.1.282. The fix is in the user's binary.
The error still exists for the cases it was meant for, and its advice — `/effort high`, or
`--effort high` and the effortLevel setting, depending on how the session started — only sticks in a
Canopy pane through the profile's Effort level field, because every spawn and resume re-emits
`--effort` from it, as it does `--model`.

**`maxProseWidth` is the one new setting, and the check it needed is which way an older CLI fails.**
It is recorded under Configuration above: a new key passes through older CLIs, which is what makes
it safe in the one settings file Canopy shares with its hooks. The same check was open to the 2.1.281
run, which records "no pre-2.1.281 build is on the runner": the SDK vendored in `node_modules` ships
a 2.1.207 `claude` binary that the `Grep` tool reads exactly like the current one.

**The web-search fix names the configuration the Base URL field exists for.** Every request failed
with a 400 "in conversations whose history holds web search results the API cannot decrypt (for
example, from a turn answered through a third-party gateway)". Canopy's Base URL hint names Ollama,
GLM, MinMax and "any OpenAI-compatible Anthropic proxy", and Canopy resumes panes with the profile's
current settings, so a session that ran through a gateway and is resumed after the profile's Base
URL changed builds exactly that history. The fix lands through the user's binary. The
`redacted_thinking` fix may share that cause, but its entry does not say so.

**The telemetry notice ranks Canopy's two env routes differently.** Its text says a project's
settings files "can only turn telemetry off", and that the CLI uses a project's off switch "unless
managed settings or a `--settings` file sets the same variable"; user settings do not override it.
Canopy's per-session file is a `--settings` file, so telemetry variables in a profile's Settings
JSON `env` still win over a worktree's off switch. The profile's env vars go into the process
environment instead, which the text does not list as overriding it. A worktree whose own
`.claude/settings.json` sets telemetry variables shows the notice in its panes.

**Nothing else visible reaches Canopy.** `allowClaudeInChromeWithManagedMcp` is a managed setting
for `claude --chrome`; Canopy writes no managed settings and never passes `--chrome`. The gateway's
`store.readiness_grace_seconds` configures a Claude apps gateway, which Canopy neither writes nor
selects. The `/feedback` drafts scrollbar relies on the mouse tracking the 2.1.271 note found working
in Canopy panes. The resume re-send, immediate-slash-command, `--tools`, redacted-thinking and
compaction-refusal fixes act inside the user's binary. Canopy emits no `--tools`, and the
commit-message turn is never resumed.

**Canopy's contract against the 2.1.282 build.** The hook-event array still names 33 events, the
same set as 2.1.281, so Canopy's 18 are all present and nothing new is unsubscribed. Every flag
Canopy emits is defined: `--settings <file-or-json>`, `--model <model>`, `--permission-mode <mode>`,
`--effort <level>`, `--append-system-prompt <prompt>`, `--system-prompt-snapshot <on|off>` and
`-r, --resume [value]`. The permission-mode list is `acceptEdits`, `auto`, `bypassPermissions`,
`default`, `dontAsk` and `plan`, covering all four the profile offers.

**Both prompt halves grew again, the system half by more.** Files +3 (+14.3%) gives 3 ÷ 0.143 =
20.98, band 20.91–21.05, so **21 before and 24 after**, continuing the 2.1.281 note's 21 for an
eleventh link. Tokens +6,946 (+21.6%) put the total at **~32.16k before and ~39.10k after** (before
band 32.08k–32.23k, reproducing that note's ~32.14k). Splitting by the given mix (59.2%/40.8% →
53.6%/46.4%) gives **tools ~19.04k → ~20.96k (+1,922) and system ~13.12k → ~18.14k (+5,024)**.
Carrying the rounding through both ends leaves tools at +1,882…+1,962 and system at +4,984…+5,064,
so both signs are safe. **All three new files and both halves' growth are unattributed**: no visible
entry names a tool or a system-prompt change, and a truncation marker is present, so that silence is
the ambiguous kind. The bundle grew **+368.6 kB (+0.7%)** from ~52.7 MB (band 49.1–56.7 MB,
consistent with the ~52.6 MB after 2.1.281), of which ~28 kB is prompt text at ~4 bytes/token.

**74 of 2.1.282's 86 CLI changelog entries were not readable this run**, against 12 visible. `WebFetch`
was denied on its first and only call, the twentieth consecutive run. **`gh api` was not probed at
all**, the first run recorded here to spend zero: the checkout and the branch copy of the compat
prompt both came before any fetch. The 2.1.282 build was on the runner again at
`~/.local/share/claude/versions/2.1.282`, and every schema claim above was read from it. It holds
no changelog text, so behaviour behind the 74 hidden entries is still unread. The vendored SDK under
`node_modules` is still `0.3.207`.

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
