# Crash reporting

> Local-only crash diagnostics that detect unclean shutdowns and present a dialog offering to file a GitHub issue.

**Status:** Stable
**Introduced:** v0.10.0
**Platforms:** All

## Overview

Canopy records crash information to a local JSON file when the app exits abnormally. On the next launch, if a crash report exists, a dialog appears showing the crash details and offering to open a pre-filled GitHub issue. No crash data is sent to any server automatically.

The crash reporter is active only in packaged (production) builds. In development mode, stack traces appear in the terminal and DevTools as usual.

## Behavior

### Sentinel-based crash detection

1. On startup, `CrashReporter.init()` checks for a sentinel file `.canopy-running` in the user data directory (`app.getPath('userData')`).
2. If the sentinel exists but no `crash-report.json` is present, the previous session exited without cleaning up. The reporter writes an `ungracefulShutdown` crash report with the message "The app did not shut down cleanly".
3. The sentinel file is then (re)written with the current process PID.
4. On graceful shutdown (`before-quit` or `window-all-closed`), `clearSentinel()` removes the sentinel file.

### Runtime crash recording

The following process-level events write a crash report immediately:

| Event                                   | Crash type           | Source                                                      |
| --------------------------------------- | -------------------- | ----------------------------------------------------------- |
| `process.on('uncaughtException')`       | `uncaughtException`  | Unhandled throw in main process                             |
| `process.on('unhandledRejection')`      | `unhandledRejection` | Unhandled promise rejection in main process                 |
| `app.on('child-process-gone')`          | `childProcessGone`   | GPU, utility, or other child process crash (non-clean-exit) |
| `webContents.on('render-process-gone')` | `rendererCrash`      | Renderer process crash (non-clean-exit)                     |

Each report overwrites any previous `crash-report.json`. Only the most recent crash is preserved.

### Crash report dialog

1. After the first window finishes loading (the `app:firstWindowReady` post-launch event), the main process reads `crash-report.json` via `getCrashReport()`.
2. If a report exists, it is pushed to the renderer over the `app:crashReport` IPC channel.
3. The renderer calls `showCrashReport(data)`, which opens the `CrashReportDialog`.
4. The main process immediately calls `clearCrashReport()` to delete the file, so the dialog does not reappear on subsequent launches.

### Dialog actions

The dialog shows: timestamp, crash type, app version, Electron version, OS, error message, and stack trace (if available). The user has two options:

1. **Dismiss** (Escape or click "Dismiss"): closes the dialog, no further action.
2. **Create issue** (Enter or click "Create issue"): opens the default browser with a pre-filled GitHub issue URL at `github.com/itsoltech/canopy-desktop/issues/new`. The issue includes labels `bug` and `crash`. Home directory paths in the stack trace are sanitized to `~/` before inclusion in the URL.

### Stack trace sanitization

Before the stack trace is included in the GitHub issue URL:

- macOS/Linux paths matching `/Users/<username>/` or `/home/<username>/` are replaced with `~/`
- Windows paths matching `<drive>:\Users\<username>\` (any drive letter A-Z) are replaced with `~/`
- The stack trace is truncated to 3,000 characters

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
  "os": "darwin 24.1.0 arm64"
}
```

The `stack` field is optional and absent for `ungracefulShutdown` reports (no error object is available in that case).

## Configuration

No user-facing configuration. The crash reporter has no preference keys or toggles. It is always active in packaged builds and inactive in development.

## Error states

| Error                    | User sees                                                                               | Cause                                                                                                                                                                     |
| ------------------------ | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Filesystem write failure | Nothing (silently swallowed)                                                            | `crash-report.json` or sentinel could not be written. Every method in `CrashReporter` wraps its body in `try/catch` to prevent the reporter itself from crashing the app. |
| Browser open failure     | Toast: "Failed to open browser. Copy the URL from your address bar to report manually." | `window.api.openExternal()` rejected when the user clicked "Create issue".                                                                                                |

## Security and privacy

Crash data never leaves the machine unless the user explicitly clicks "Create issue". The GitHub issue URL is constructed client-side and opened in the user's browser, where they can review and edit it before submitting.

Home directory paths are stripped from stack traces before they are placed in the URL. The crash report file is stored in the OS-standard user data directory and is readable only by the current user (default OS permissions).

## Source files

- Main: `src/main/crash/CrashReporter.ts`
- IPC wiring: `src/main/index.ts` (crash handler registration near line 381, push to renderer near line 810)
- Preload: `src/preload/index.ts` (`onCrashReport` listener)
- Dialog: `src/renderer/src/components/dialogs/CrashReportDialog.svelte`
- Dialog state: `src/renderer/src/lib/stores/dialogs.svelte.ts` (`CrashReportData`, `showCrashReport`)
