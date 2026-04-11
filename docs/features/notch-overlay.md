# Notch overlay

> Display AI agent session status in a floating overlay anchored to the screen's top edge.

**Status:** Stable
**Introduced:** v0.10.0
**Platforms:** macOS, Windows

## Overview

The notch overlay shows a compact status indicator for all active AI agent sessions. On macOS, it anchors to the physical notch area of the display. On Windows, it renders a simulated notch at the top center of the primary screen. The overlay is always-on-top, transparent, and click-through by default, becoming interactive only on hover.

Each session row displays the workspace name, git branch, an optional title (first user prompt), and the current status with a color-coded icon. Clicking a row focuses the corresponding Canopy window and selects that agent pane. The overlay hides when no sessions are active.

The overlay uses a separate BrowserWindow with its own preload script (`notch.js`) and HTML entry (`notch.html`). It renders on a black background to blend with the physical notch hardware. All colors are fixed (not themed) because the overlay always sits on a solid black surface.

## Behavior

### Initialization

1. On app launch, the `NotchOverlayManager` checks the `notch.enabled` preference. If `false`, no window is created.
2. On macOS, `hasNotch()` checks whether the menu bar height exceeds 28 logical pixels (indicating a notch display). If false, the overlay is not created.
3. On Windows, `hasNotch()` returns true unconditionally (simulated notch).
4. A frameless, transparent, always-on-top `BrowserWindow` is created at the top center of the primary display. Dimensions: 560 x 650 pixels (oversized so CSS animations can expand within it).
5. On macOS, the window type is `panel` (does not appear in Cmd+Tab, does not steal focus). It is set as visible on all workspaces, including fullscreen. After creating a panel window, the app dock icon is restored (panel windows cause macOS to hide it) and the previously focused window is re-focused.
6. On Windows, a warmup pass briefly shows the empty overlay off-screen for 200ms to force GPU compositing, preventing stutter on first hover.
7. Mouse events are set to pass through (`setIgnoreMouseEvents(true, { forward: true })`).
8. The manager subscribes to `AgentSessionManager` events: `statusChange` and `sessionDestroyed`.

### Status updates

1. When any agent session changes status, the `statusChange` listener fires.
2. The manager stores the session's `NotchSessionStatus` in an internal map keyed by `ptySessionId`.
3. If the status is `ended`, the session is removed from the map.
4. The manager checks whether the transition is "peek-worthy" (see below). If so, and if the session is not the focused agent pane in the focused window, the session ID is added to `pendingPeekIds`.
5. `pushState()` sends the full session list plus notch dimensions to the overlay renderer via `notch:stateUpdate`.
6. If sessions exist, the overlay window is shown (inactive). If the session list is empty, the overlay hides.

### Peek-worthy transitions

A session status change triggers an automatic 4-second peek when:

- The new status is `waitingPermission` (agent needs user approval)
- The new status is `error`
- The new status is `idle` and the previous status was an active state (thinking, toolCalling, compacting, waitingPermission, error) -- indicating that the agent finished its work

The peek is suppressed if the session is currently the focused agent pane in the focused Canopy window (the user is already looking at it).

### Hover interaction

1. When the mouse enters the overlay area, the renderer sends `notch:setMouseIgnore { ignore: false }` so the overlay captures clicks.
2. The overlay expands vertically to show a scrollable list of sessions (max 12 visible rows, each 48px tall).
3. During a peek, hovering locks the peek and prevents auto-collapse so the user can interact.
4. When the mouse leaves, a 300ms delay starts before collapsing. Mouse ignore is re-enabled.
5. Users with `prefers-reduced-motion` see no animations (transitions are set to `0s`).

### Clicking a session row

1. User clicks a row in the expanded overlay.
2. The renderer calls `notch:focusSession` with `windowId` and `ptySessionId`.
3. The main process locates the target Canopy window, restores it if minimized, focuses it, and sends `agent:focusSession` to the window's renderer to switch to that agent pane.

### Aggregate status indicator

The collapsed overlay shows two wing icons flanking the notch gap. Their color reflects the highest-priority status across all sessions:

| Priority | Condition                                                 | Color             | Icon              |
| -------- | --------------------------------------------------------- | ----------------- | ----------------- |
| 1        | Any session is `waitingPermission`                        | `#f87171` (red)   | ShieldAlert       |
| 2        | Any session is `error`                                    | `#f87171` (red)   | ShieldAlert       |
| 3        | Any session is `thinking`, `toolCalling`, or `compacting` | `#f59e0b` (amber) | Loader (spinning) |

Per-row colors differ from the aggregate: `compacting` shows `#60a5fa` (blue) in individual rows, while the aggregate maps it to amber via the `working` status.
| 4 | All sessions are `idle` | `#4ade80` (green) | TerminalSquare |

### Display changes

The manager listens for `display-metrics-changed` events. If the display no longer has a notch (e.g. external monitor), the overlay hides. If the notch returns, the overlay repositions to the new center.

## Configuration

| Preference key  | Values               | Default   |
| --------------- | -------------------- | --------- |
| `notch.enabled` | `"true"` / `"false"` | `"false"` |

The toggle is available in Settings (under Notch) and in the first-launch wizard (Features step). Changing the preference calls `window.api.setNotchEnabled(boolean)` which sends `notch:setEnabled` to the main process, triggering `initialize()` or `dispose()` on the overlay manager.

## Session status types

| Status              | Label in overlay                   | Meaning                                                      |
| ------------------- | ---------------------------------- | ------------------------------------------------------------ |
| `idle`              | "Idle" (or "Finished" during peek) | Agent is waiting for input                                   |
| `thinking`          | "Thinking..."                      | Agent is generating a response                               |
| `toolCalling`       | Tool name (e.g. "Read")            | Agent is executing a tool                                    |
| `compacting`        | "Compacting..."                    | Agent is compressing context                                 |
| `waitingPermission` | Detail text or "Permission needed" | Agent needs user approval to proceed                         |
| `error`             | Detail text or "Error"             | Agent encountered an error                                   |
| `ended`             | "Ended"                            | Session has terminated (removed from overlay on next update) |

## Source files

- Manager: `src/main/notch/NotchOverlayManager.ts`
- Types: `src/main/notch/types.ts`
- Preload: `src/preload/notch.ts`
- Overlay component: `src/renderer/src/components/notch/NotchOverlay.svelte`
- Row component: `src/renderer/src/components/notch/NotchNotificationRow.svelte`
- Preferences UI: `src/renderer/src/components/preferences/NotchPrefs.svelte`
- Main preload (toggle): `src/preload/index.ts` (`setNotchEnabled`)
