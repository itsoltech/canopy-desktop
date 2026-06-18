# Crash reporting

> Local-only crash diagnostics that detect unclean shutdowns and present a dialog offering to file a GitHub issue.

**Status:** Stable
**Introduced:** v0.10.0
**Platforms:** All

## Overview

Canopy records crash information to a local JSON file when the app exits abnormally. On the next launch, if a crash report exists, a dialog appears showing the crash details and offering to copy a report and open a GitHub issue page. No crash data is sent to any server automatically.

The crash reporter is active only in packaged (production) builds. In development mode, stack traces appear in the terminal and DevTools as usual.

## Behavior

### Sentinel-based crash detection

1. On startup, `CrashReporter.init()` checks for a sentinel file `.canopy-running` in the user data directory (`app.getPath('userData')`).
2. If the sentinel exists but no `crash-report.json` is present, the previous session exited without cleaning up. The reporter writes an `ungracefulShutdown` crash report.
3. On macOS only, before writing the report the reporter also scans `~/Library/Logs/DiagnosticReports/` (and `…/Retired/`) for a recent `Canopy-*.ips` dump whose mtime is at or after the previous sentinel mtime. If one is found, the reporter parses the header and body JSON, extracts the triggered thread's symbolicated frames from the `.ips`, and attaches them to the report as `stack`, along with structured fields in `nativeCrash` (exception type, codes, termination reason, triggered thread name, incident id, and native stack). The `.ips` file path is not exposed in the report. The `errorMessage` becomes e.g. `Native crash: EXC_BREAKPOINT (SIGTRAP) in CrBrowserMain` instead of the generic "did not shut down cleanly" string. If no matching `.ips` is found, the report falls back to the generic message with no stack. Windows and Linux native-dump recovery are not implemented — the same generic fallback applies there.
4. The sentinel file is then (re)written with the current process PID.
5. On graceful shutdown, `clearSentinel()` removes the sentinel file. It is called from three places, in order of preference:
   - The main `before-quit` handler (synchronous path, line ~1027 in `index.ts`).
   - The updater-install branch of `before-quit` (line ~934), before Squirrel relaunches.
   - A `process.on('exit')` fallback registered next to `CrashReporter.init()` — fires on any normal Node exit including SIGTERM/SIGINT and the `before-quit` async dialog paths (active-session confirm, tmux close-policy) that `preventDefault()` and return early without reaching the main cleanup block. This fallback cannot run on SIGKILL / Task Manager force-kill.

### Runtime crash recording

The following process-level events write a crash report immediately:

| Event                                   | Crash type           | Source                                                      |
| --------------------------------------- | -------------------- | ----------------------------------------------------------- |
| `process.on('uncaughtException')`       | `uncaughtException`  | Unhandled throw in main process                             |
| `process.on('unhandledRejection')`      | `unhandledRejection` | Unhandled promise rejection in main process                 |
| `app.on('child-process-gone')`          | `childProcessGone`   | GPU, utility, or other child process crash (non-clean-exit) |
| `webContents.on('render-process-gone')` | `rendererCrash`      | Renderer process crash (non-clean-exit)                     |

Each report overwrites any previous `crash-report.json`. Only the most recent crash is preserved.

For renderer crashes on macOS, the reporter also looks for a matching recent `Canopy Helper (Renderer)-*.ips` dump. If the native dump is not yet available when the renderer process exits, `getCrashReport()` attempts the lookup again before sending the report to the renderer.

### Crash report dialog

1. After the first window finishes loading (the `app:firstWindowReady` post-launch event), the main process reads `crash-report.json` via `getCrashReport()`.
2. If a report exists, it is pushed to the renderer over the `app:crashReport` IPC channel.
3. The renderer calls `showCrashReport(data)`, which opens the `CrashReportDialog`.
4. The main process immediately calls `clearCrashReport()` to delete the file, so the dialog does not reappear on subsequent launches.

### Dialog actions

The dialog shows: timestamp, crash type, app version, Electron version, OS, error message, stack trace (if available), renderer reason/exit code (for renderer crashes), native exception summary (if available), and the exact Markdown report that will be copied. The user has two options:

1. **Dismiss** (Escape or click "Dismiss"): closes the dialog, no further action.
2. **Copy report and open issue** (Enter or click the button): copies the Markdown report to the clipboard, then opens the default browser at `github.com/itsoltech/canopy-desktop/issues/new` with a neutral title, labels `bug` and `crash`, and a placeholder body asking the user to paste the copied report. Diagnostic fields are not placed in the issue URL.

### Diagnostic sanitization

Before crash data is written to `crash-report.json`, shown in the dialog, copied to the clipboard, or sent over IPC:

- macOS/Linux paths matching `/Users/<username>/` or `/home/<username>/` are replaced with `~/`
- Windows paths matching `<drive>:\Users\<username>\` (any drive letter A-Z) are replaced with `~/`
- HTTP(S) URLs are replaced with `[redacted]`
- common secret forms are replaced with `[redacted]`, including bearer tokens, `Authorization: Bearer ...`, `password: ...`, `api_key = ...`, and uppercase environment-style values
- stack fields are truncated to 4,000 characters

## Crash report format

The `crash-report.json` file stored in `app.getPath('userData')` contains:

```json
{
  "timestamp": "2024-12-15T10:30:00.000Z",
  "type": "uncaughtException",
  "errorMessage": "Cannot read properties of undefined",
  "stack": "TypeError: Cannot read properties of undefined\n    at ...",
  "appVersion": "0.11.0",
  "electronVersion": "33.2.1",
  "os": "darwin 24.1.0 arm64",
  "process": "main"
}
```

The `stack` field is optional. Renderer crash reports may include renderer process details:

```json
{
  "type": "rendererCrash",
  "process": "renderer",
  "renderer": {
    "reason": "crashed",
    "exitCode": 5
  }
}
```

For `ungracefulShutdown` and renderer crash reports, native crash details are absent unless a matching macOS `.ips` native dump was recovered, in which case the reporter also populates an optional `nativeCrash` object:

```json
{
  "nativeCrash": {
    "exceptionType": "EXC_BREAKPOINT (SIGTRAP)",
    "exceptionCodes": "0x0000000000000001, 0x0000000113060e1c",
    "terminationReason": "Trace/BPT trap: 5 (by exc handler)",
    "triggeredThread": "CrBrowserMain",
    "incidentId": "A1CE2AD8-CF3F-4E78-A2F1-7AA8328E8857",
    "stack": "  0  Canopy                            main + 0"
  }
}
```

## Configuration

No user-facing configuration. The crash reporter has no preference keys or toggles. It is always active in packaged builds and inactive in development.

## Error states

| Error                    | User sees                                                             | Cause                                                                                                                                                                     |
| ------------------------ | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Filesystem write failure | Nothing (silently swallowed)                                          | `crash-report.json` or sentinel could not be written. Every method in `CrashReporter` wraps its body in `try/catch` to prevent the reporter itself from crashing the app. |
| Clipboard write failure  | Toast: "Failed to copy crash report to clipboard"                     | `navigator.clipboard.writeText()` rejected when the user clicked "Copy report and open issue"; the browser is not opened.                                                 |
| Browser open failure     | Toast: "Failed to open browser. Crash report is copied to clipboard." | `window.api.openExternal()` rejected after the report was copied.                                                                                                         |

## Security and privacy

Crash data never leaves the machine automatically. The user must explicitly click "Copy report and open issue" to place the report on the clipboard. The GitHub issue URL is constructed client-side with only neutral query params: title, placeholder body, and labels. The user decides whether to paste the copied report and submit the issue in the browser.

Home directory paths, URLs, and common secret formats are stripped from diagnostics before they are persisted or displayed. Native `.ips` source paths are not included in the public report. The crash report file is stored in the OS-standard user data directory and is readable only by the current user (default OS permissions).

## Source files

- Main: `src/main/crash/CrashReporter.ts`
- Crash diagnostic sanitizer: `src/main/crash/sanitizeCrashDiagnostic.ts`
- Native dump reader (macOS `.ips`): `src/main/crash/NativeCrashReader.ts`
- IPC wiring: `src/main/index.ts` (crash handler registration near line 381, push to renderer near line 810)
- Preload: `src/preload/index.ts` (`onCrashReport` listener)
- Dialog: `src/renderer/src/components/dialogs/CrashReportDialog.svelte`
- Dialog state: `src/renderer/src/lib/stores/dialogs.svelte.ts` (`CrashReportData`, `showCrashReport`)
