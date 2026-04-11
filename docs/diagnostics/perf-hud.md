# Performance HUD

> Real-time CPU and RAM usage display in the status bar, with zero overhead when disabled.

**Status:** Stable
**Introduced:** v0.11.0
**Platforms:** All

## Overview

The performance HUD shows the app's aggregate CPU percentage and memory usage as a compact readout in the right side of the status bar (e.g. `12% · 384 MB`). It is opt-in: disabled by default, toggled on in Settings > General. When disabled, no sampling runs and no IPC messages are sent.

The HUD is useful for spotting runaway PTY sessions, memory leaks from long-lived agent sessions, or GPU process spikes. Clicking the readout opens Settings > General where the toggle lives.

A separate developer-only diagnostics mode is available behind the `CANOPY_PERF=1` environment variable. That mode exposes additional IPC endpoints for profiling internals and is not covered by the user-facing HUD toggle.

## Behavior

### Enabling the HUD

1. The user opens Settings > General and checks the "Performance HUD" toggle.
2. The preference `perf.hud.enabled` is set to `'true'`.
3. A reactive `$effect` in `StatusBar.svelte` detects the change and calls `enablePerfHud()`.
4. The renderer store sends `perf:hud:start` to the main process.
5. `PerfHudService.subscribe()` registers the requesting `WebContents` as a subscriber and starts the sampling interval if not already running.
6. If a previous sample exists, it is sent immediately so the HUD does not show an empty state for up to one second.

### Sampling loop

1. The main process calls `app.getAppMetrics()` once per second (1000 ms interval).
2. CPU usage from all Electron processes (main, renderer, GPU, utility) is summed. The raw value is per-core (Chromium convention), so on an 8-core machine a fully busy app reports ~800%. `PerfHudService` normalizes this to 0-100% of the whole machine by dividing by `os.cpus().length` and clamping to 100.
3. Memory (`workingSetSize` in KB) from all processes is summed and converted to MB.
4. If the rounded CPU and memory values are identical to the last sent payload, no IPC message is emitted. This avoids unnecessary DOM updates when the app is idle.
5. Changed values are sent to all subscribed `WebContents` on the `perf:hud:metrics` channel.

### Renderer display

1. The `perfHud.svelte.ts` store receives metrics via the `perf:hud:metrics` IPC listener and updates `perfHudState.metrics`.
2. `StatusBar.svelte` renders the metrics as `{cpu}% · {memMb} MB` in the right section of the status bar.
3. The button has an `aria-label` of `App CPU {cpu}%, RAM {memMb} MB` and a tooltip "App CPU / RAM -- click to open settings".

### Disabling the HUD

1. The user unchecks the toggle in Settings > General.
2. `disablePerfHud()` is called: the IPC listener is removed, `perfHudState.metrics` is set to `null`, and `perf:hud:stop` is sent to the main process.
3. `PerfHudService.unsubscribe()` removes the `WebContents` from the subscriber map. If no subscribers remain, `clearInterval` stops the sampling loop.
4. The status bar conditionally hides the readout when `perfHudState.metrics` is null.

### Edge cases

- **Window reload**: The `WebContents` ID persists across renderer reloads (no `destroyed` event fires). `PerfHudService.subscribe()` detects the duplicate ID and re-sends the last sample so the reloaded renderer is not blank.
- **Window close**: A `destroyed` listener on the `WebContents` auto-unsubscribes the window. If it was the last subscriber, the interval stops.
- **App shutdown**: `PerfHudService.shutdown()` clears all subscribers, stops the interval, and removes all event listeners.

## Configuration

| Preference key     | Values               | Default              | Description                                                                |
| ------------------ | -------------------- | -------------------- | -------------------------------------------------------------------------- |
| `perf.hud.enabled` | `'true'` / `'false'` | `'false'` (disabled) | Toggles the status bar CPU/RAM readout and the main-process sampling loop. |

### Developer diagnostics (CANOPY_PERF=1)

Launching with `CANOPY_PERF=1` enables two additional IPC endpoints exposed through the preload bridge:

| Endpoint           | Returns                                                                                                                                                                       |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `perf:diagnostics` | Object with `ptySessionCount`, `wsBridgeCount`, `agentSessionCount`, `gitWatcherCount`, `windowCount`, `uptime`, `heapUsed`, `rss`, and `marks` (performance timeline marks). |
| `perf:ipcLog`      | Array of `{ channel, size, ts, dir }` entries logged since the last call. The log is drained on read.                                                                         |

These endpoints are gated at the preload level: when `CANOPY_PERF` is not `'1'`, the properties are not added to the `window.api` object. The main-process IPC interceptor that records traffic is also only installed when `CANOPY_PERF=1`.

## Error states

No user-visible errors. `app.getAppMetrics()` is a synchronous Electron API that does not throw. If a subscribed `WebContents` is destroyed between ticks, the `isDestroyed()` guard skips sending to it, and the `destroyed` listener removes it on the next event loop turn.

## Source files

- Main: `src/main/perf/PerfHudService.ts`
- IPC handlers: `src/main/index.ts` (`perf:hud:start`, `perf:hud:stop` near line 694; `perf:diagnostics`, `perf:ipcLog` near line 672)
- Preload: `src/preload/index.ts` (`perfHud` object and conditional `perfDiagnostics`/`perfIpcLog`)
- Store: `src/renderer/src/lib/stores/perfHud.svelte.ts`
- Status bar: `src/renderer/src/components/layout/StatusBar.svelte`
- Settings toggle: `src/renderer/src/components/preferences/GeneralPrefs.svelte`
