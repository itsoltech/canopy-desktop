# Canopy - Developer Terminal Workstation

Cross-platform desktop application for managing developer projects through CLI tools. Uses restty (libghostty-vt WASM + WebGPU/WebGL2) for terminal emulation. Project-centric UI with git worktree awareness, built-in tool launcher, and Claude Code integration.

**Stack**: Electron + Svelte 5 + TypeScript + restty
**Target**: macOS, Linux, Windows. Direct distribution (DMG, AppImage, NSIS installer).
**Package name**: `canopy`

---

## 1. Terminal emulation layer

### 1.1 restty

`npm i restty`. Library wraps libghostty-vt compiled to WASM. Renders via WebGPU with WebGL2 fallback. Provides VT100-compatible terminal emulation, multi-pane layouts, 485+ built-in Ghostty themes, Kitty image protocol, and custom shader support.

### 1.2 PTY bridge

Main process runs node-pty. Renderer connects via WebSocket served from main process.

```
[Renderer: restty] <--WebSocket--> [Main: ws server] <--node-pty--> [shell/tool process]
```

Main process creates a local WebSocket server per PTY session. Each PTY gets a unique port or path. restty's `connectPty(url)` connects to it.

### 1.3 Theme switching

Theme changes apply immediately to all running terminals via `restty.applyTheme()`. Supports built-in themes (`getBuiltinTheme()`) and custom Ghostty theme files (`parseGhosttyTheme()`). Default theme hardcoded in Phase 1; theme picker UI added in Phase 10.

---

## 2. Application architecture

### 2.1 Process model

- **Main process**: Electron main. Manages windows, PTY sessions (node-pty), WebSocket PTY bridge, file system access, git operations, SQLite database, Claude hook server.
- **Renderer process**: Svelte 5 app. UI rendering, restty terminal instances, user interaction.
- **IPC**: Electron contextBridge + ipcRenderer/ipcMain for structured communication between processes.

### 2.2 Window model

Single window application. One workspace active at a time. Switch workspaces via command palette, welcome dashboard, or `Cmd+O`/`Ctrl+O` (open folder). `Cmd+N`/`Ctrl+N` unbound.

When switching workspaces, all previous workspace PTY sessions keep running in background. Switching back restores UI state with live terminals.

If the opened folder is not a git repository, the sidebar worktree section is hidden. A single entry shows the folder name. Tool launching uses the folder as working directory. Git-related command palette entries are disabled.

### 2.3 Layout structure

```
+---------------------------------------------------+
| Toolbar: [workspace name] [branch] [+] [gear]     |
+----------+----------------------------+-----------+
| Left     | Terminal Area              | Right     |
| Sidebar  |                            | Inspector |
| (toggle) | Tab Bar (shrink+overflow)  | (toggle)  |
|          | +--------+--------+------+ |           |
| WORKTREES| | claude | lazygit| shell| | Claude    |
| * main   | +--------+--------+------+ | session   |
|   feat/x | |                        | | state     |
|   fix/y  | |  Terminal content      | |           |
|          | |  (restty canvas)       | | Permission|
| TOOLS    | |                        | | prompts   |
| > claude | |                        | |           |
| > codex  | +------------------------+ | Convo     |
| > lazygit|                            | preview   |
| > shell  |                            |           |
+----------+----------------------------+-----------+
```

**Left sidebar**: collapsible (`Cmd+B`/`Ctrl+B`). Contains worktree list and tool launcher. Tool entries show badge with count of running instances in current worktree.
**Right panel (Claude Inspector)**: toggleable (`Cmd+Shift+I`/`Ctrl+Shift+I`). Claude Code session panel. Only shown for Claude tabs in v1.
**Tab bar**: shrinks tabs to min-width, then shows overflow `...` dropdown for remaining tabs.

### 2.4 Keyboard shortcut boundary

All `Cmd+key` (macOS) / `Ctrl+key` (Linux/Windows) combinations intercepted by Canopy (app shortcuts). Everything else passes through to the terminal. No configurable passthrough, no mode switching.

`Cmd+K`/`Ctrl+K` captured by the app for command palette. Users use `Ctrl+L` for terminal clear.

---

## 3. Data model

### 3.1 Persistence

SQLite via better-sqlite3 in main process. Database at:

- macOS: `~/Library/Application Support/canopy/canopy.db`
- Linux: `~/.config/canopy/canopy.db`
- Windows: `%APPDATA%/canopy/canopy.db`

Sequential migrations. Each schema change is a new migration, never altering existing ones.

Tables (Phase 2 migration):

- `workspaces` - id (TEXT PK), path (TEXT UNIQUE), name, is_git_repo (BOOL), last_opened (DATETIME), cached_branch, cached_dirty (BOOL), cached_ahead_behind (TEXT), cached_worktree_count (INT)
- `tool_definitions` - id (TEXT PK), name, command, args_json, icon, category, is_custom (BOOL)
- `preferences` - key (TEXT PK), value (TEXT)

Table (Phase 10 migration):

- `workspace_layouts` - workspace_id (FK), worktree_path (TEXT), layout_json (TEXT), updated_at. JSON structure: array of tabs, each with recursive SplitNode tree where leaves contain tool_id and args. Keyed by `worktree_path`.

Recent workspaces: `SELECT * FROM workspaces ORDER BY last_opened DESC LIMIT 10`. Cached git status columns refreshed in background.

### 3.2 Session restore on app restart

Persist: which workspace was open, which worktree selected, which tools were running (tab layout + split arrangement). On restart: reopen workspace, recreate tabs, relaunch tool processes. New PTY processes, no scrollback history carried over. For tools with own session persistence (Claude Code session resume), the tool handles continuity.

### 3.3 Core types

```typescript
interface Workspace {
  id: string
  path: string // git repo root or plain directory
  name: string // directory name
  isGitRepo: boolean
  worktrees: Worktree[]
  lastOpened: Date | null
}

interface Worktree {
  id: string // runtime-generated, not persisted
  path: string // stable identifier, used as key in workspace_layouts
  branch: string
  isMain: boolean
  tabs: Tab[]
}

interface Tab {
  id: string
  name: string // display name, e.g. "claude" or "claude #2"
  worktreeId: string
  rootSplit: SplitNode
}

type SplitNode =
  | { type: 'leaf'; session: TerminalSession }
  | { type: 'horizontal'; first: SplitNode; second: SplitNode; ratio: number }
  | { type: 'vertical'; first: SplitNode; second: SplitNode; ratio: number }

interface TerminalSession {
  id: string
  tool: ToolDefinition
  worktreeId: string
  isRunning: boolean
  ptyId: string | null // reference to main process PTY
  wsUrl: string | null // WebSocket URL for restty connection
}

interface ToolDefinition {
  id: string // "claude", "lazygit", etc.
  name: string
  command: string // literal binary name; shell tool uses resolved $SHELL
  args: string[]
  icon: string // icon identifier
  category: 'ai' | 'git' | 'system' | 'shell'
}

interface ClosedTabEntry {
  tool: ToolDefinition
  worktreePath: string
  closedAt: Date
}
```

Notes:

- `Worktree.id` is runtime-only (regenerated from `git worktree list`). The `path` field is the stable identifier.
- Shell tool: resolve `$SHELL` env var (macOS/Linux) or use PowerShell/cmd on Windows.
- `ClosedTabEntry` stack: in-memory only, cleared on restart, capped at 20 per worktree.

---

## 4. Session management

### 4.1 Per-worktree session isolation

Each worktree maintains its own set of terminal sessions. Switching worktrees in the sidebar swaps the entire tab bar. Sessions in other worktrees continue running but are not visible.

### 4.2 Tool instances

Multiple instances of the same tool allowed per worktree. Clicking a tool in sidebar spawns a new session. Tabs show tool name with suffix when duplicated: `claude`, `claude #2`, `claude #3`.

### 4.3 Tool availability

On launch (and on preferences change), scan `$PATH` for each configured tool binary via `which` (Unix) or `where` (Windows). Unavailable tools greyed out in sidebar with "Not found in PATH" tooltip.

### 4.4 Tool launching

When launching a tool:

1. Resolve binary path via `which <command>` (shell tool: resolve `$SHELL` directly)
2. Set working directory to selected worktree path via node-pty cwd
3. Set environment: inherit user env + `TERM_PROGRAM=canopy`
4. Apply tool-specific launch hooks: additional CLI arguments and environment variables from registered handlers (e.g., claude gets `--settings <path>` and `CANOPY_HOOK_PORT`)
5. Create PTY via node-pty, start WebSocket bridge, return wsUrl for restty `connectPty()`

### 4.5 Session lifecycle

- **Launch**: create PTY (main process), start WS bridge, renderer calls `restty.connectPty(wsUrl)`
- **Exit**: show inline banner "Process exited (code N)" with restart button
- **Tab close**: send SIGHUP to process group (Unix) or kill process tree (Windows), clean up PTY + WS
- **App quit**: send SIGHUP/kill to all active sessions, wait briefly for cleanup

### 4.6 Split panes

Per-tab splits via restty's built-in `splitActivePane()`. Each tab can be split independently.

- `Cmd+D`/`Ctrl+D`: split vertical (`restty.splitActivePane("vertical")`)
- `Cmd+Shift+D`/`Ctrl+Shift+D`: split horizontal (`restty.splitActivePane("horizontal")`)
- `Cmd+W`/`Ctrl+W` on focused pane: close pane (`restty.closePane(id)`), close tab if last pane
- `Cmd+Option+Arrow`/`Ctrl+Alt+Arrow`: move focus between panes
- Click pane to focus
- Drag divider to resize
- Splitting spawns a new shell session in the same worktree
- Maximum depth: 4 levels (practical limit)

When the last tab in the active worktree is closed, auto-open a new shell tab.

---

## 5. Predefined tools

| ID       | Name        | Command                | Category | Icon       |
| -------- | ----------- | ---------------------- | -------- | ---------- |
| claude   | Claude Code | claude                 | ai       | brain      |
| codex    | Codex       | codex                  | ai       | sparkles   |
| gemini   | Gemini CLI  | gemini                 | ai       | wand       |
| opencode | OpenCode    | opencode               | ai       | code       |
| lazygit  | LazyGit     | lazygit                | git      | git-branch |
| htop     | htop        | htop                   | system   | activity   |
| btop     | btop        | btop                   | system   | bar-chart  |
| shell    | Shell       | (resolved from $SHELL) | shell    | terminal   |

Shell tool resolves `$SHELL` at launch (macOS/Linux) or defaults to PowerShell (Windows). User can add custom tool definitions in preferences.

---

## 6. Git integration

### 6.1 Repository detection

On workspace open (main process via simple-git or raw git commands):

- `git rev-parse --show-toplevel` to find repo root
- `git worktree list --porcelain` to enumerate worktrees
- `git branch --show-current` for active branch
- chokidar watch on `.git/` (branch changes, index updates) and `.git/worktrees/` (worktree additions/removals)

### 6.2 Worktree management

**Creation** (guided flow):

1. Fetch remotes (`git fetch --all`)
2. Pick base branch (from local + remote branches)
3. Name new branch (auto-suggest from pattern)
4. Choose directory (default: sibling of main worktree)
5. Optionally run setup commands post-creation (user-configured)
6. `git worktree add -b <branch> <path> <base>`

**Removal** (with safety):

1. Check for uncommitted changes (`git status --porcelain`)
2. Check for unmerged commits (`git log <branch> --not --remotes`)
3. Warn user with details. Allow force-remove.
4. Offer to delete associated branch (local + remote) if merged
5. `git worktree remove <path>` + optional branch cleanup

### 6.3 Command palette git commands

Available via Command Palette (`Cmd+K`/`Ctrl+K`, see Section 9):

- `git commit` - opens commit message input
- `git push` - confirm dialog showing branch + remote + commit count
- `git pull` - runs `git pull --rebase` (configurable)
- `git fetch` - runs immediately
- `git stash` / `git stash pop` - immediate
- `git branch create` - name input + base branch picker
- `git branch delete` - confirm dialog, checks merge status

Destructive operations show confirmation dialog. Safe operations execute immediately.

---

## 7. Welcome dashboard

Shown on cold start (no workspace open) or when user closes current workspace.

### 7.1 Content

**Recent workspaces** (last 10):

- Repository name, path
- Current branch name
- Dirty/clean indicator
- Ahead/behind remote count
- Number of worktrees
- Last accessed date

Git status fetched in background per repo. Show immediately with stale/no data, update as results arrive.

**Quick actions**: Open folder (native dialog), Open from path (text input).

### 7.2 Interaction

Click workspace to open. Right-click for context menu: open in file manager, copy path, remove from recent.

---

## 8. Claude Code integration

### 8.1 Hook mechanism

Canopy installs Claude Code hooks only for sessions it spawns. External Claude sessions unaffected.

**Per-session isolation via `--settings` flag**: when Canopy launches `claude`, it passes `--settings <path>` pointing to a session-specific JSON file. Hook arrays from `--settings` are concatenated with user's existing hooks.

**Session lifecycle**:

1. Generate session UUID
2. Write settings JSON to `<userData>/canopy/claude-hooks/session-<uuid>.json`
3. Create IPC channel (main process) for hook communication
4. Launch `claude --settings <path>` via node-pty with env: `CANOPY_HOOK_PORT=<port>`, `TERM_PROGRAM=canopy`
5. On SessionEnd or process exit: cleanup IPC, delete settings JSON

**Startup sweep**: on app launch, delete orphaned files in claude-hooks dir where no matching session exists.

### 8.2 Hooks used by Canopy

| Hook                  | Purpose                                                   | Blocking | Timeout  |
| --------------------- | --------------------------------------------------------- | -------- | -------- |
| SessionStart          | Inject workspace context via `additionalContext`          | No       | 10s      |
| PreToolUse            | Update Inspector to "tool calling", show tool name + args | No       | 10s      |
| PostToolUse           | Record tool result in recent tool calls list              | No       | 10s      |
| PostToolUseFailure    | Record tool failure                                       | No       | 10s      |
| **PermissionRequest** | **Show approve/deny UI, hold until user decides**         | **Yes**  | **600s** |
| Stop                  | Transition Inspector to "idle"                            | No       | 10s      |
| StopFailure           | Show error state                                          | No       | 10s      |
| SubagentStart         | Track subagent spawn in Inspector                         | No       | 10s      |
| SubagentStop          | Track subagent completion                                 | No       | 10s      |
| Notification          | Forward to tab badges + OS notifications                  | No       | 10s      |
| SessionEnd            | Cleanup: stop server, delete settings file                | No       | 5s       |

### 8.3 Hook handler script

Bundled script at `resources/canopy-claude-hook.sh` (Unix) / `resources/canopy-claude-hook.cmd` (Windows). Thin forwarder using curl to POST hook JSON to a localhost HTTP endpoint run by the main process. All logic lives in the Electron main process.

```bash
#!/bin/bash
# Forward hook JSON to canopy via HTTP, return response
[ -z "$CANOPY_HOOK_PORT" ] && exit 0
INPUT=$(cat)
curl -s -X POST "http://127.0.0.1:${CANOPY_HOOK_PORT}/hook" \
  -H "Content-Type: application/json" \
  -d "$INPUT" 2>/dev/null || exit 0
```

Behavior:

- If `$CANOPY_HOOK_PORT` unset or server unreachable: exit 0 (pass-through)
- For PermissionRequest: curl blocks on read until Canopy sends response
- For all others: app responds immediately

### 8.4 IPC protocol

HTTP server on localhost (random port per session). JSON request/response.

**Request** (hook script to Canopy):

```json
{
  "session_id": "abc123",
  "hook_event_name": "PreToolUse",
  "transcript_path": "/path/to/transcript.jsonl",
  "cwd": "/project/root",
  "permission_mode": "default",
  "tool_name": "Bash",
  "tool_input": { "command": "npm test" }
}
```

**Response** for PermissionRequest:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": { "behavior": "allow" }
  }
}
```

Denial:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": { "behavior": "deny", "message": "Denied by user in Canopy" }
  }
}
```

SessionStart context injection:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "Working in canopy workspace 'my-project', worktree 'feat/auth', branch: feat/auth."
  }
}
```

### 8.5 PermissionRequest flow

```
Claude wants to run Bash("rm -rf node_modules")
  |
Claude Code fires PermissionRequest hook
  |
canopy-claude-hook.sh reads JSON from stdin
  |
Script POSTs to http://127.0.0.1:<port>/hook
curl blocks waiting for response
  |
Main process receives event, sends to renderer via IPC
Renderer updates ClaudeSessionState -> waitingPermission
Inspector panel shows: tool name, arguments, [Approve] [Deny] buttons
If Claude tab not focused: orange tab badge + OS notification
  |
User clicks Approve or Deny
  |
Renderer sends decision via IPC to main process
Main process responds to pending HTTP request
  |
curl receives response, outputs to stdout, exits 0
  |
Claude Code reads hook output, proceeds or blocks tool
```

### 8.6 Inspector panel state machine

Right sidebar panel, toggled with `Cmd+Shift+I`/`Ctrl+Shift+I`. Only active for Claude Code tabs. Switching between Claude tabs swaps to that tab's state.

**States and transitions**:

```
inactive --(SessionStart)--> starting
starting --(PreToolUse)--> toolCalling
starting --(Stop)--> idle
idle --(PreToolUse)--> toolCalling
toolCalling --(PreToolUse)--> toolCalling (new tool)
toolCalling --(PermissionRequest)--> waitingPermission
toolCalling --(PostToolUse)--> toolCalling (records result)
toolCalling --(Stop)--> idle
waitingPermission --(user approves)--> toolCalling
waitingPermission --(user denies)--> toolCalling
* --(StopFailure)--> error
error --(Stop)--> idle
* --(SessionEnd)--> ended
ended --(cleanup)--> inactive
```

**Data model** (Svelte 5 runes):

```typescript
interface ClaudeSessionState {
  status: ClaudeStatus
  sessionId: string | null
  startTime: Date | null
  recentToolCalls: ToolCallRecord[] // ring buffer, last 50
  activeSubagents: SubagentRecord[]
  permissionMode: string | null
}

type ClaudeStatus =
  | { type: 'inactive' }
  | { type: 'starting' }
  | { type: 'idle' }
  | { type: 'toolCalling'; toolName: string; toolInput: Record<string, unknown> }
  | {
      type: 'waitingPermission'
      toolName: string
      toolInput: Record<string, unknown>
      requestId: string
    }
  | { type: 'error'; errorType: string; details: string }
  | { type: 'ended'; reason: string }
```

### 8.7 SessionStart context injection

When SessionStart fires, Canopy responds with `additionalContext`:

```
Working in canopy workspace 'my-project', worktree 'feat/auth' (branch: feat/auth).
Project root: /Users/nix/projects/my-project.
```

### 8.8 Notifications

When Claude tab is not focused:

- **Tab badge** (unread dot): on PostToolUse or Stop
- **Orange tab badge**: on PermissionRequest (user action required)
- **OS notification**: optional, for PermissionRequest events. Clicking focuses the Claude tab.

### 8.9 Claude Code hooks reference

Complete list of all 22 Claude Code hook events:

| Event              | Matcher           | Key input fields                              | Can block (exit 2) |
| ------------------ | ----------------- | --------------------------------------------- | ------------------ |
| SessionStart       | source            | source, model                                 | No                 |
| UserPromptSubmit   | (none)            | prompt                                        | Yes                |
| PreToolUse         | tool name         | tool_name, tool_input                         | Yes                |
| PermissionRequest  | tool name         | tool_name, tool_input, permission_suggestions | Yes                |
| PostToolUse        | tool name         | tool_name, tool_input, tool_response          | No                 |
| PostToolUseFailure | tool name         | tool_name, tool_input, error, is_interrupt    | No                 |
| Notification       | notification_type | message, title, notification_type             | No                 |
| SubagentStart      | agent type        | agent_id, agent_type                          | No                 |
| SubagentStop       | agent type        | agent_id, agent_type, last_assistant_message  | Yes                |
| Stop               | (none)            | stop_hook_active, last_assistant_message      | Yes                |
| StopFailure        | error type        | error, error_details                          | No                 |
| TeammateIdle       | (none)            | teammate_name, team_name                      | Yes                |
| TaskCompleted      | (none)            | task_id, task_subject, task_description       | Yes                |
| InstructionsLoaded | load reason       | file_path, memory_type, load_reason           | No                 |
| ConfigChange       | config source     | source, file_path                             | Yes                |
| WorktreeCreate     | (none)            | name                                          | Yes                |
| WorktreeRemove     | (none)            | worktree_path                                 | No                 |
| PreCompact         | trigger           | trigger, custom_instructions                  | No                 |
| PostCompact        | trigger           | trigger, compact_summary                      | No                 |
| Elicitation        | MCP server name   | mcp_server_name, message, mode                | Yes                |
| ElicitationResult  | MCP server name   | mcp_server_name, content                      | Yes                |
| SessionEnd         | exit reason       | reason                                        | No                 |

All hooks receive common fields: `session_id`, `transcript_path`, `cwd`, `permission_mode`, `hook_event_name`.

---

## 9. Command palette (Cmd+K / Ctrl+K)

Full command palette, accessible from anywhere.

### 9.1 Search categories

- **Tools**: launch any configured tool in current worktree
- **Worktrees**: switch, create new (guided flow), remove (safety flow)
- **Git**: commit, push, pull, fetch, stash, branch operations
- **App**: toggle sidebar, toggle inspector, open preferences, switch workspace, switch theme
- **Tabs**: switch to open tab by name, close tab

### 9.2 Behavior

Fuzzy search across all categories. Results grouped by category. Enter to execute. Esc to dismiss. `>` prefix to filter to app commands. `git ` prefix to filter to git commands.

---

## 10. Fonts

restty handles font rendering. Configure via `fontSources` in constructor and `setFontSources()` at runtime.

Priority order for auto-detection:

1. JetBrainsMono Nerd Font (local)
2. FiraCode Nerd Font (local)
3. Hack Nerd Font (local)
4. restty default CDN fonts (fallback)

User overrides in preferences: font family, font size (default 13, set via `restty.setFontSize()`).

---

## 11. URL scheme

Register `canopy://` URL scheme via Electron protocol handler.

- `canopy://open?path=/path/to/repo` - open workspace
- `canopy://run?tool=claude&path=/path/to/repo` - open workspace and launch tool
- `canopy://run?tool=lazygit&worktree=feat/x&path=/path/to/repo` - launch in specific worktree

---

## 12. Error handling

- **Git network errors**: show inline error in command palette result toast. No UI blocking.
- **Tool binary crashes**: trigger exit banner (Section 4.5) with exit code/signal info and restart button.
- **Disk full** (SQLite write fails): show dialog, degrade gracefully (continue without persistence).
- **Worktree path deleted externally**: detected via chokidar, remove from sidebar with notification. Close associated tabs.
- **Non-existent workspace path on restore**: skip workspace on restart, show note in welcome dashboard.

---

## 13. Preferences

Accessible via `Cmd+,`/`Ctrl+,` or command palette.

Sections:

- **General**: default shell, scan roots for dashboard, startup behavior
- **Appearance**: theme (picker from 485+ built-in themes + custom), font family, font size, sidebar default state, renderer (auto/webgpu/webgl2)
- **Tools**: list of configured tools, add/edit/remove custom tools, default args per tool
- **Git**: default pull strategy (rebase/merge), post-worktree-creation commands, branch name patterns
- **Shortcuts**: read-only list of keyboard shortcuts

---

## 14. Keyboard shortcuts

| Shortcut (macOS / Linux,Win)      | Action                                   |
| --------------------------------- | ---------------------------------------- |
| Cmd+K / Ctrl+K                    | Command palette                          |
| Cmd+T / Ctrl+T                    | New shell tab in current worktree        |
| Cmd+W / Ctrl+W                    | Close current pane (or tab if last pane) |
| Cmd+Shift+T / Ctrl+Shift+T        | Reopen last closed tab                   |
| Cmd+1-9 / Ctrl+1-9                | Switch to tab N                          |
| Cmd+Shift+[ / Ctrl+Shift+[        | Previous tab                             |
| Cmd+Shift+] / Ctrl+Shift+]        | Next tab                                 |
| Cmd+D / Ctrl+D                    | Split pane vertical                      |
| Cmd+Shift+D / Ctrl+Shift+D        | Split pane horizontal                    |
| Cmd+Option+Arrow / Ctrl+Alt+Arrow | Move focus between split panes           |
| Cmd+B / Ctrl+B                    | Toggle left sidebar                      |
| Cmd+Shift+I / Ctrl+Shift+I        | Toggle Claude Inspector                  |
| Cmd+O / Ctrl+O                    | Open workspace (folder picker)           |
| Cmd+, / Ctrl+,                    | Preferences                              |

---

## 15. Build configuration

### 15.1 Electron setup

- electron-forge or electron-builder for packaging
- Svelte 5 with Vite for renderer
- TypeScript throughout
- Context isolation enabled, nodeIntegration disabled
- preload script exposes IPC API via contextBridge

### 15.2 Dependencies

| Dependency     | Purpose                                                |
| -------------- | ------------------------------------------------------ |
| restty         | Terminal emulation (libghostty-vt WASM, WebGPU/WebGL2) |
| node-pty       | PTY management in main process                         |
| ws             | WebSocket server bridging PTY to restty                |
| better-sqlite3 | SQLite database access                                 |
| chokidar       | File system watching (.git/ changes)                   |
| svelte (5.x)   | UI framework (renderer)                                |
| vite           | Build tooling                                          |
| electron       | Desktop shell                                          |

### 15.3 Bundled resources

`resources/canopy-claude-hook.sh` (Unix) and `resources/canopy-claude-hook.cmd` (Windows) bundled in app package. Referenced by absolute path in per-session hook settings.

---

## 16. File structure

```
canopy-desktop/
  package.json
  electron.vite.config.ts
  tsconfig.json

  src/
    main/
      index.ts                  # Electron main entry
      pty/
        PtyManager.ts           # node-pty lifecycle
        WsBridge.ts             # WebSocket server per PTY
      db/
        Database.ts             # better-sqlite3 setup, migrations
        WorkspaceStore.ts       # CRUD for workspaces
        PreferencesStore.ts     # key-value preferences
        LayoutStore.ts          # tab/split serialization
      git/
        GitRepository.ts        # status, branch, worktree detection
        GitWorktreeManager.ts   # create, remove, list
        GitCommandRunner.ts     # palette git operations
      claude/
        ClaudeHookServer.ts     # HTTP server for hook communication
        ClaudeSessionManager.ts # per-session settings + lifecycle
      ipc/
        handlers.ts             # ipcMain handler registration
      tools/
        ToolLauncher.ts         # binary resolution, env setup
        ToolRegistry.ts         # predefined + custom tools, PATH scanning

    preload/
      index.ts                  # contextBridge API exposure

    renderer/
      index.html
      main.ts                   # Svelte app mount
      App.svelte                # Root component

      lib/
        terminal/
          TerminalInstance.svelte  # restty wrapper component
          sessionStore.svelte.ts  # terminal session state (runes)
        claude/
          claudeState.svelte.ts   # Claude session state machine (runes)
          ClaudeInspector.svelte   # right sidebar panel
        stores/
          workspace.svelte.ts     # workspace + worktree state
          tabs.svelte.ts          # tab management state
          preferences.svelte.ts   # preferences state
        ipc/
          api.ts                  # typed IPC calls to main process

      components/
        layout/
          MainLayout.svelte       # three-column layout shell
          Toolbar.svelte
        sidebar/
          Sidebar.svelte
          WorktreeSection.svelte
          ToolSection.svelte
        terminal/
          TabBar.svelte
          TerminalContainer.svelte
          SplitPane.svelte
        palette/
          CommandPalette.svelte
          PaletteAction.ts
        welcome/
          WelcomeDashboard.svelte
        preferences/
          PreferencesView.svelte
          GeneralPrefs.svelte
          AppearancePrefs.svelte
          ToolPrefs.svelte
          GitPrefs.svelte
          ShortcutsPrefs.svelte
        worktree/
          CreateWorktreeModal.svelte
          RemoveWorktreeModal.svelte

  resources/
    canopy-claude-hook.sh       # Unix hook forwarder
    canopy-claude-hook.cmd      # Windows hook forwarder
    icon.png                    # App icon
```

---

## 17. Implementation phases

### Phase 1: Terminal foundation

Electron project scaffolding with Svelte 5 + Vite. restty integration in renderer. node-pty in main process. WebSocket bridge connecting node-pty to restty. Verify: launch app, get shell, type commands, all keys work.

### Phase 2: Data foundation

better-sqlite3 setup + migrations. Core tables: `workspaces`, `tool_definitions`, `preferences`. TypeScript types (Workspace, Worktree, Tab, SplitNode, TerminalSession, ToolDefinition). PreferencesStore. ToolRegistry with predefined tools and `$SHELL` resolution. IPC handlers for database access.

### Phase 3: Workspace + worktrees

Git detection (GitRepository). Open folder flow. Sidebar with worktree list. Per-worktree session isolation. chokidar watcher for `.git/` and `.git/worktrees/`. Non-git folder support. Three-column layout shell in MainLayout.svelte.

### Phase 4: Tool launcher + tabs

Tab bar with shrink+overflow. PATH scanning for tool availability. Launch tools into worktree context. Session lifecycle (exit banner, restart). ClosedTabEntry stack. Sidebar tool section with running instance badge. Wire all tab shortcuts.

### Phase 5: Split panes

restty `splitActivePane()` integration. Keyboard shortcuts for split/navigate/close. Click-to-focus. Drag to resize. Per-tab split state.

### Phase 6: Command palette

Cmd+K/Ctrl+K overlay. Fuzzy search across tools, worktrees, app actions, tabs. `>` prefix for app commands.

### Phase 7: Git operations + worktree management

Git commands in palette. Confirmation dialogs for destructive ops. Guided worktree creation flow. Removal with safety checks and branch cleanup.

### Phase 8: Welcome dashboard

Recent workspaces from database. Background git status fetch. Rich display. Quick actions (open folder, open path).

### Phase 9: Claude Code integration

Bundle hook scripts. ClaudeSessionManager: write per-session hook config JSON, pass `--settings` flag. ClaudeHookServer: HTTP server in main process, accept hook events, dispatch to renderer via IPC. claudeState: reactive state machine. ClaudeInspector: right sidebar panel with approve/deny buttons. PermissionRequest blocking flow. Tab badges. OS notifications. SessionStart context injection. Test with real Claude session.

### Phase 10: Persistence, preferences, polish

`workspace_layouts` table. Layout persistence. Restore tabs and splits on restart. Preferences UI (all sections). Custom tool CRUD. Theme picker + `restty.applyTheme()`. Font auto-detection. URL scheme handler. Renderer selection (auto/webgpu/webgl2). Shader stage support in preferences.

---

## 18. Verification

Each phase gate:

1. Type in terminal. All keys work. Shell is responsive. WebGPU/WebGL2 rendering active.
2. Database created on launch. Predefined tools loaded. Preferences read/write works.
3. Open git repo with worktrees. Sidebar shows worktrees. Switch worktree swaps tabs. Non-git folder hides worktree section.
4. Click tool in sidebar. New tab opens. Tool runs in correct worktree dir. Unavailable tools greyed out. Cmd+Shift+T reopens closed tab.
5. Cmd+D splits pane. Both panes functional. Focus navigation works. Drag divider resizes.
6. Cmd+K opens palette. Search tools, worktrees, app actions. Launch tool from palette.
7. `git push` from palette shows confirmation. Create worktree from guided flow. Remove with branch cleanup.
8. Cold start shows dashboard. Recent repos with git status.
9. Launch claude from Canopy. Inspector shows session state. Tool calls appear in real-time. Approve/deny permission from Inspector. Orange tab badge when permission pending on unfocused tab. SessionStart injects workspace context.
10. Quit and reopen. Same workspace, tabs, splits restored. Tools relaunched. Theme picker works. Font auto-detected. Preferences UI works. Custom tool added via preferences appears in sidebar. URL scheme opens workspace.
