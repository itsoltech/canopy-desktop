# Terminal / PTY

> Each tab in Canopy runs a pseudo-terminal backed by `node-pty`, with output streamed to the renderer over a local WebSocket.

**Status:** Stable
**Introduced:** v0.1.0
**Platforms:** All (macOS, Linux, Windows)

## Overview

The terminal is the primary surface of the application. When a user opens a new shell tab, the main process spawns a PTY (`node-pty`) running the user's login shell and creates a WebSocket bridge so the renderer can display output via xterm.js. Each PTY session gets a UUID, and the renderer connects to `ws://127.0.0.1:<port>/<sessionId>` to read and write data.

On macOS and Linux the default shell is read from `$SHELL` and launched with `--login`. On Windows it falls back to `powershell.exe`. Non-exe commands on Windows are wrapped through `cmd.exe /c` so `.cmd`/`.bat` wrappers resolve correctly.

The terminal component uses xterm.js with WebGL rendering (disposed when a tab is inactive, reattached when active to conserve GPU memory), ligatures support, web links detection, and a progress bar addon. Font rendering waits for `document.fonts.ready` before initializing so glyph metrics are accurate.

Tmux integration (dev builds only) allows creating new tmux sessions or attaching to existing ones. Tmux is disabled in production builds (`isAvailable()` returns false when `!is.dev`). In dev, sessions run on a dedicated socket (`canopy-dev`) with a minimal config that disables the status bar and unbinds all keys to avoid conflicts with xterm keybindings.

## Behavior

### Opening a new shell tab

1. User presses the new-tab shortcut or clicks the add button.
2. Renderer calls `window.api.spawnPty({ cols, rows, cwd })`.
3. Main process spawns a PTY via `PtyManager.spawn()`, assigns a UUID session ID.
4. `WsBridge.create()` attaches to the PTY data stream and returns a `ws://` URL.
5. Renderer receives `{ sessionId, wsUrl }` and creates a `TerminalInstance` component.
6. `TerminalInstance` waits for fonts to load, creates an xterm.js `Terminal`, then connects to the WebSocket URL.
7. PTY output arrives as WebSocket messages, gets buffered, and flushed to xterm on the next `requestAnimationFrame`.
8. User keystrokes flow from xterm's `onData` callback through the WebSocket back to the PTY.

### WebSocket reconnection

1. If the WebSocket closes unexpectedly, the renderer schedules a reconnect with exponential backoff starting at 500ms, doubling up to 8000ms.
2. During reconnection, a `reconnecting` status is set on the connection state store.
3. On reconnect, the client passes `?offset=<receivedChars>` so the bridge replays only missed history.
4. After 30 consecutive failed attempts, the status changes to `disconnected` and reconnection stops.

### Scrollback and history

The WsBridge maintains a circular buffer of up to 1 MB of PTY output per session. When a new WebSocket client connects (or reconnects), it receives history from the requested offset. xterm.js maintains a separate scrollback buffer of 5000 lines.

### Terminal resize

1. A `ResizeObserver` on the terminal container fires on dimension changes.
2. Resize events are debounced (80ms) to prevent WebGL texture churn during continuous window dragging.
3. After the debounce settles, `FitAddon.fit()` recalculates cols/rows and fires `term.onResize`.
4. The resize callback calls `window.api.resizePty(sessionId, cols, rows)` which forwards to `PtyManager.resize()`.
5. When the user clicks back into a terminal after a remote peer resized the PTY, the component always sends `resizePty` on `pointerdown`/`focus` to reclaim the desktop dimensions.

### File drag and drop

1. User drags files onto the terminal.
2. A drop overlay appears ("Drop files").
3. On drop, each file's path is shell-escaped (single quotes with escaped inner quotes) and pasted into the terminal.

### URL handling

Detected URLs in terminal output are clickable. Behavior depends on the `urlOpenMode` preference:

- `ask` (default): shows a toast asking the user what to do.
- `canopy`: opens the URL in the built-in browser tool.
- `system`: opens the URL in the system default browser.

### Keyboard shortcuts

- Shift+Enter: inserts a newline (sends `ESC` + `CR`).
- Cmd+Backspace (macOS): kills the line (sends `Ctrl-U`).
- Ctrl+V (Windows/Linux): paste.
- Ctrl+C (Windows/Linux): copy if text is selected, otherwise sends `^C`.
- Ctrl+Z is blocked in AI tool terminals to prevent unrecoverable `SIGTSTP`.

### Tab close with active processes

1. When a user closes a tab, the system checks whether the PTY has child processes (`pgrep -P <pid>` on Unix, `wmic` on Windows).
2. For AI tool tabs, it checks whether the agent status is in an active state (thinking, tool calling, compacting, waiting for permission).
3. If active processes are found, a confirmation dialog appears: "This tab has N running processes that will be terminated."
4. On confirmation, all PTYs in the tab's split tree are killed.

### Tmux session lifecycle

1. User requests a new tmux tab or attaches to an existing session.
2. `TmuxManager.newSession()` creates a detached tmux session on the `canopy` socket with a generated name (`canopy-<workspacePrefix>-<uuid>`).
3. A PTY is spawned running `tmux attach-session -t <name>`.
4. The tmux config disables the status bar, unbinds all keys, sets escape-time to 0, and sets history-limit to 10000.
5. On tab close with `killTmux=true`, the tmux session is killed via `tmux kill-session`.
6. On tab close with `killTmux=false` (detach), only the PTY wrapper is killed; the tmux session persists and can be reattached.

## Configuration

| Preference key | Type                                | Default                    | Description                |
| -------------- | ----------------------------------- | -------------------------- | -------------------------- |
| `theme`        | string                              | `"Default"`                | Terminal color theme name  |
| `fontSize`     | string                              | `"13"`                     | Font size in pixels        |
| `fontFamily`   | string                              | JetBrains Mono + fallbacks | Font family stack          |
| `urlOpenMode`  | `"ask"` \| `"canopy"` \| `"system"` | `"ask"`                    | How to handle clicked URLs |

### Built-in themes

Default, Dracula, Monokai, Solarized Dark, Solarized Light, Nord, One Dark, Gruvbox Dark, Tokyo Night, Catppuccin Mocha, GitHub Dark, Rosé Pine.

The terminal theme's `background` color is applied to the app chrome (`--c-bg` CSS variable), so switching themes changes the entire application appearance.

### Environment variables

Every PTY session inherits the user's login environment (resolved via `getLoginEnv()`) with these additions:

| Variable       | Value            |
| -------------- | ---------------- |
| `TERM_PROGRAM` | `canopy`         |
| `COLORTERM`    | `truecolor`      |
| `TERM`         | `xterm-256color` |

## Error states

| Error                      | User sees                               | Cause                                       |
| -------------------------- | --------------------------------------- | ------------------------------------------- |
| WebSocket disconnected     | "reconnecting" overlay on terminal      | WebSocket server or PTY process crashed     |
| WebSocket permanently lost | "disconnected" overlay                  | 30 consecutive reconnect attempts failed    |
| WebGL context loss         | Transparent fallback to canvas renderer | GPU driver issue or resource pressure       |
| PTY spawn failure          | Tab fails to open                       | Shell binary not found or permission denied |

## Source files

- PTY manager: `src/main/pty/PtyManager.ts`
- Tmux manager: `src/main/pty/TmuxManager.ts`
- WebSocket bridge: `src/main/pty/WsBridge.ts`
- Terminal component: `src/renderer/src/lib/terminal/TerminalInstance.svelte`
- Connection state: `src/renderer/src/lib/terminal/connectionState.svelte.ts`
- Themes: `src/renderer/src/lib/terminal/themes.ts`
- Tab management: `src/renderer/src/lib/stores/tabs.svelte.ts`
- Preload (PTY/Tmux API): `src/preload/index.ts`
