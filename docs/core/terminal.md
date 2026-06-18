# Terminal / PTY

> Each tab in Canopy runs a pseudo-terminal backed by `node-pty`, with output streamed to the renderer over Electron IPC.

**Status:** Stable
**Introduced:** v0.1.0
**Platforms:** All (macOS, Linux, Windows). Shell child-process close warnings are
macOS/Linux-only; Windows still shows AI agent-busy close warnings.

## Overview

The terminal is the primary surface of the application. When a user opens a new shell tab, the main process spawns a PTY (`node-pty`) running the user's login shell and registers it with `TerminalStreamService` so the renderer can display output via xterm.js. Each PTY session gets a UUID, and the renderer subscribes to `pty-stream:*` IPC events through the preload bridge while user input is written through domain IPC methods.

On macOS and Linux the default shell is read from `$SHELL` and launched with `--login`. On Windows it falls back to `powershell.exe`. Non-exe commands on Windows are wrapped through `cmd.exe /c` so `.cmd`/`.bat` wrappers resolve correctly.

The terminal component uses xterm.js with WebGL rendering (disposed when a tab is inactive, reattached when active to conserve GPU memory), ligatures support, web links detection, and a progress bar addon. Font rendering waits for `document.fonts.ready` before initializing so glyph metrics are accurate.

Tmux integration (dev builds only) allows creating new tmux sessions or attaching to existing ones. Tmux is disabled in production builds (`isAvailable()` returns false when `!is.dev`). In dev, sessions run on a dedicated socket (`canopy-dev`) with a minimal config that disables the status bar and unbinds all keys to avoid conflicts with xterm keybindings.

## Behavior

### Opening a new shell tab

1. User presses the new-tab shortcut or clicks the add button.
2. Renderer asks the main-owned tab command API to open the shell for the selected worktree.
3. Main process validates the sender owns that worktree, then spawns a PTY via `PtyManager.spawn()`
   and assigns a UUID session ID.
4. `TerminalStreamService.register()` attaches to the PTY data stream and stores bounded replay history.
5. Renderer receives the session ID and creates a `TerminalInstance` component.
6. `TerminalInstance` waits for fonts to load, creates an xterm.js `Terminal`, then subscribes through `window.api.subscribePtyData(sessionId, receivedChars, ...)`.
7. PTY output arrives as IPC stream events, gets buffered, and flushed to xterm on the next `requestAnimationFrame`.
8. User keystrokes flow from xterm's `onData` callback through `window.api.writePty(sessionId, data)` back to the PTY.

### IPC stream resubscription

1. The renderer owns a preload subscription cleanup function for the active PTY stream.
2. If the app pauses terminal streams for lock/suspend, the renderer drops the subscription and keeps its `receivedChars` offset.
3. On resume, the renderer creates a fresh subscription with the current offset so `TerminalStreamService` replays only missed history.
4. If a write or subscription setup fails, the connection state store can show `disconnected`.

### Screen lock and wake handling

Canopy keeps PTY processes alive while the OS session is locked or suspended, but pauses terminal
stream reconnects so the renderer does not try to catch up while Chromium is throttled. Screen-lock
pause uses Electron `lock-screen`/`unlock-screen` events where supported (macOS and Windows);
Linux builds rely on `suspend`/`resume` for this lifecycle handling.

1. Main process tracks terminal stream pause reasons from Electron `powerMonitor` events.
   `lock-screen` adds a screen-lock pause reason and `suspend` adds a system-suspend pause reason.
2. `unlock-screen` clears only the screen-lock reason. `resume` clears only the system-suspend
   reason. If both reasons are active, terminal streams resume only after both have cleared.
3. On the first transition into paused state, main broadcasts `terminal-stream:state` and disconnects
   current terminal stream subscribers. PTY processes keep running.
4. While paused, `TerminalInstance` drops its PTY data subscription and does not create a new
   subscription. It also clears transient connection status so the UI does not remain stuck on stale
   state.
5. When the effective pause state clears, visible terminal panes resubscribe and request missed
   history through their existing offset.
6. If Electron emits `resume` without a prior tracked `suspend`, Canopy still force-closes terminal
   stream subscribers to preserve the defensive wake behavior for stale subscriptions.
7. If a wake clears `suspend` but only `lock-screen` remains, main starts a 30 second watchdog.
   A normal `unlock-screen` cancels it and resumes streams immediately; if the OS never delivers
   `unlock-screen`, the watchdog clears the stale screen-lock pause so terminal streaming can
   recover without restarting Canopy.

### Scrollback and history

`TerminalStreamService` maintains a bounded buffer of up to 1 MB of PTY output per session. When a renderer or remote forwarder subscribes, it receives history from the requested offset. xterm.js maintains a separate scrollback buffer of 5000 lines.

Replayed or bursty output is flushed to xterm in bounded chunks on animation frames instead of
writing an entire backlog in one frame. This keeps the renderer responsive after long lock/unlock
or wake intervals where terminal output accumulated in the bridge history.

### Terminal resize

1. A `ResizeObserver` on the terminal container fires on dimension changes.
2. Resize events are debounced (80ms) to prevent WebGL texture churn during continuous window dragging.
3. After the debounce settles, `FitAddon.fit()` recalculates cols/rows and fires `term.onResize`.
4. The resize callback calls `window.api.resizePty(sessionId, cols, rows)` which forwards to `PtyManager.resize()`.
5. When the user clicks back into a terminal after a remote peer resized the PTY, the component always sends `resizePty` on `pointerdown`/`focus` to reclaim the desktop dimensions.

### Cursor handling on Windows (ConPTY)

ConPTY repaints arrive as a stream of CUP+char runs, so without intervention the xterm cursor visibly traces the path of every redraw. To suppress this on Windows only:

1. Each output burst is bracketed with DECTCEM hide (`\x1b[?25l`) before the data and a debounced restore (`\x1b[?25h`) 80ms after the last burst.
2. The burst data is scanned for the last `\x1b[?25[lh]` sequence the TUI emitted; if the TUI ended with the cursor explicitly hidden (e.g. `claude` while thinking, `vim` during paste), the restore timer is skipped so the TUI's intent is honored.
3. On disposal, any pending restore timer is cleared and the cursor is re-shown so no hidden-cursor state leaks across tab close.

macOS and Linux are unaffected — `writeBurst` falls through to a plain scroll-preserving write without DECTCEM bracketing.

### Pane tab strip controls

In split layouts, each pane shows a strip above the pane body with the pane title and actions:

- **Detach pane to tab**: removes that pane from the current split and opens it as a new top-level tab.
- **Close pane**: closes only that pane. On macOS/Linux, panes with running shell child
  processes use the same termination confirmation flow before closing. On Windows, shell panes do
  not raise this warning; AI agent-busy warnings still apply.

Clicking the strip also focuses the pane.

### Pane dragging via strip

1. The user drags a pane by pressing and moving on its strip (left mouse button; action buttons are excluded).
2. If dropped over another pane's edge zone, the pane is inserted on the corresponding side (left/right/top/bottom).
3. While dragging over the main tab bar, hovering a tab for 300ms switches to that tab so the pane can be dropped there.
4. Dropping over empty space in the tab bar detaches the pane into a new tab (same result as the detach button).

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

1. On macOS/Linux, when a user closes a shell tab, the system checks whether the PTY has
   child processes with `pgrep -P <pid>`.
2. On Windows, shell tabs do not run child-process close-warning checks because the available
   process-tree probes are too expensive for responsive close preflights.
3. For AI tool tabs on all platforms, it checks whether the agent status is in an active state
   (thinking, tool calling, compacting, waiting for permission).
4. If active shell processes on macOS/Linux or busy AI agents on any platform are found, a
   confirmation dialog appears before the PTY is killed.
5. On confirmation, all PTYs in the tab's split tree are killed.

Closing all tabs for a worktree uses the same safety path. The renderer first aggregates active
process warnings across tabs, then runs the unsaved-editor preflight for every open tab. If the
user cancels either prompt, the worktree removal/detach operation that requested the close is
cancelled as well. Closing the final tab leaves the worktree with no tabs; Canopy no longer opens
a replacement shell automatically.

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

The terminal theme's `background` color is applied to the app chrome (`--color-bg` CSS variable), so switching themes changes the entire application appearance.

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
| Terminal stream disconnected | "disconnected" overlay on terminal      | IPC stream subscription or PTY process failed |
| Terminal stream paused       | No error state; connection status clears | Screen is locked or system is suspended       |
| WebGL context loss         | Transparent fallback to canvas renderer | GPU driver issue or resource pressure       |
| PTY spawn failure          | Tab fails to open                       | Shell binary not found or permission denied |

## Source files

- Power lifecycle handling: `src/main/index.ts`
- PTY manager: `src/main/pty/PtyManager.ts`
- Terminal stream service: `src/main/pty/TerminalStreamService.ts`
- Tmux manager: `src/main/pty/TmuxManager.ts`
- Terminal component: `src/renderer/src/lib/terminal/TerminalInstance.svelte`
- Connection state: `src/renderer/src/lib/terminal/connectionState.svelte.ts`
- Themes: `src/renderer/src/lib/terminal/themes.ts`
- Tab management: `src/renderer/src/lib/stores/tabs.svelte.ts`
- Preload (PTY/Tmux API and terminal stream state): `src/preload/index.ts`

## IPC channels

| Channel                    | Direction       | Purpose                                                                      |
| -------------------------- | --------------- | ---------------------------------------------------------------------------- |
| `pty:resize`               | Renderer invoke | Resize a PTY after xterm fit changes.                                        |
| `pty:kill`                 | Renderer invoke | Terminate a PTY session, optionally killing the tmux session.                |
| `pty:write`                | Renderer invoke | Write user input to the PTY.                                                 |
| `pty:getDimensions`        | Renderer invoke | Read the current PTY cols/rows.                                              |
| `pty:exit`                 | Main event      | Notify renderer that a PTY exited.                                           |
| `pty:resized`              | Main event      | Broadcast PTY size changes to renderers and remote-control forwarding.       |
| `pty-stream:subscribe`     | Renderer invoke | Subscribe a renderer to PTY output from a retained character offset.         |
| `pty-stream:unsubscribe`   | Renderer invoke | Drop a PTY output subscription.                                              |
| `pty-stream:data`          | Main event      | Send replay or live PTY output to one logical subscriber.                    |
| `pty-stream:closed`        | Main event      | Notify one logical subscriber that the PTY stream ended.                     |
| `terminal-stream:getState` | Renderer invoke | Read whether terminal stream reconnects are currently paused.                |
| `terminal-stream:state`    | Main event      | Notify terminal panes when lock/suspend pauses or resumes stream reconnects. |
