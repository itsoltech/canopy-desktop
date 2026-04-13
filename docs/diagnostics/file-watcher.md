# File watcher

> Recursive filesystem monitoring that pushes granular change events to the renderer for the file tree sidebar, diff pane, changes panel, and editor.

**Status:** Stable
**Introduced:** v0.10.0
**Platforms:** All

## Overview

When the user opens a workspace, Canopy starts a native filesystem watcher on the workspace root directory. The watcher uses `@parcel/watcher`, which delegates to OS-native APIs: FSEvents on macOS, inotify on Linux, ReadDirectoryChangesW on Windows. Events are debounced, deduplicated, and pushed to the renderer as `files:changed` messages.

**This watcher is responsible for the visible file tree and project files.** It is distinct from the [Git watcher](../core/git.md) which specifically monitors the internal `.git/` directory for repository state changes.

Four renderer consumers react to these events: the file tree sidebar refreshes affected directories, the diff pane re-reads changed files, the changes panel updates its git status list, and the editor pane detects external modifications or deletions of the open file.

The watcher enforces a hardcoded safety ignore list at the native level (directories like `node_modules`, `.git`, `dist`). User-configurable ignore patterns are applied separately in each renderer consumer. This split lets the sidebar hide entries the user does not want to see while the diff and changes panels still observe all git-tracked changes.

## Behavior

### Starting the watcher

1. The renderer calls `files:watch` with the workspace root path (absolute).
2. The main process validates that the path is absolute and belongs to one of the window's registered workspaces.
3. Any existing watcher for that window is disposed first (one watcher per window).
4. A new `FileTreeWatcher` is created with the workspace root and a callback that sends `files:changed` events to the requesting `WebContents`.
5. `FileTreeWatcher.start()` calls `@parcel/watcher.subscribe()` with the `SAFETY_IGNORE_PATTERNS` list.
6. On success, the watcher reference is stored in `WindowManager` keyed by the `WebContents` ID.
7. On failure, the `WatchStartFailed` error is thrown to the renderer via the IPC rejection.

### Event processing

1. `@parcel/watcher` delivers batches of native events (create, update, delete) to the `handleEvents` callback.
2. Each event's absolute path is converted to a forward-slash relative path from the workspace root.
3. Events are deduplicated by `type:path` key in a pending map.
4. A 50 ms debounce timer coalesces rapid bursts (e.g. `npm install`, git checkout).
5. When the timer fires, the accumulated batch is delivered to the `onChange` callback, which sends it to the renderer as `files:changed` with the `repoRoot` and the event array.

### Event types

| Native event | Mapped type | Meaning                        |
| ------------ | ----------- | ------------------------------ |
| `create`     | `add`       | New file or directory appeared |
| `update`     | `change`    | Existing file content modified |
| `delete`     | `unlink`    | File or directory removed      |

### Renderer consumers

**File tree sidebar** (`FileTreeSection.svelte`): calls `fileTree.applyFileEvents()`, which filters events through user ignore patterns and re-reads only the parent directories of affected files that are currently expanded.

**Diff pane** (`DiffPane.svelte`): debounces file change events by 200 ms, then refreshes the diff view and triggers a visual pulse.

**Changes panel** (`ChangesPanel.svelte`): debounces file change events by 200 ms, then refreshes the git status list.

**Editor pane** (`EditorPane.svelte`): checks each event against the currently open file path. On `unlink`, marks the file as deleted. On `change`, reloads the file content.

### User ignore patterns

1. The user edits ignore patterns in Settings (stored as `files.ignorePatterns`, a JSON array of strings).
2. The renderer calls `files:updateIgnorePatterns` to persist the new list.
3. The main-process watcher is NOT restarted. User patterns are applied per-consumer in the renderer.
4. The `fileTree.svelte.ts` store's `isIgnoredByUser()` function matches patterns against path segments. Plain names (e.g. `logs`) match any segment exactly. Patterns with a `/` (e.g. `logs/**`) match by their first segment.
5. The diff pane and changes panel receive all events regardless of user ignore patterns, matching GitHub diff semantics.

### Stopping the watcher

1. The renderer calls `files:unwatch` (e.g. when switching workspaces or closing the project).
2. `WindowManager.disposeFileWatcher()` calls `FileTreeWatcher.stop()`.
3. `stop()` cancels the debounce timer, clears pending events, and calls `subscription.unsubscribe()` to release the native watcher.

### Default ignore patterns

The `files:getDefaultIgnorePatterns` IPC handler returns the `DEFAULT_IGNORE_PATTERNS` list, used by the "Reset to defaults" button in Settings. The current list:

`node_modules`, `.git`, `dist`, `build`, `.next`, `.nuxt`, `.output`, `.svelte-kit`, `out`, `target`, `.venv`, `venv`, `__pycache__`, `.pytest_cache`, `.DS_Store`, `coverage`, `.turbo`, `.cache`, `.parcel-cache`

Dotfiles like `.env`, `.gitignore`, `.prettierrc` are intentionally not in this list. Developers need to see them in the sidebar.

## Configuration

| Preference key         | Values                | Default                           | Description                                                                                                                   |
| ---------------------- | --------------------- | --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `files.ignorePatterns` | JSON array of strings | unset (falls back to empty array) | User-editable list of directory/file names to hide in the sidebar. Does not affect the native watcher or diff/changes panels. |

## Error states

| Error                  | User sees                                                               | Cause                                                                                                                                                                                  |
| ---------------------- | ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WatchStartFailed`     | IPC rejection: "Failed to start watcher at {path}: {message}"           | Native watcher could not subscribe. Common causes: path does not exist, permission denied, inotify limit exceeded (Linux).                                                             |
| `WatchStopFailed`      | IPC rejection: "Failed to stop watcher at {path}: {message}"            | Native watcher unsubscribe failed. Rare; typically means the subscription was already invalidated.                                                                                     |
| Native transient error | Console warning: `[FileTreeWatcher] native error for {root}: {message}` | The OS reported a transient error (directory temporarily unavailable, permission denied mid-watch, inotify limit hit). The batch is dropped; the next event delivery retries normally. |
| Invalid `repoRoot`     | IPC rejection: "Invalid repoRoot: must be an absolute path string"      | Renderer passed a relative path or non-string to `files:watch`.                                                                                                                        |
| Path access violation  | IPC rejection (from `validatePathAccess`)                               | The requested path is not under any workspace registered to the calling window.                                                                                                        |

## Source files

- Main: `src/main/fileWatcher/FileTreeWatcher.ts`
- Error types: `src/main/fileWatcher/errors.ts`
- Default patterns: `src/main/fileWatcher/defaults.ts`
- IPC handlers: `src/main/ipc/handlers.ts` (`files:watch`, `files:unwatch`, `files:updateIgnorePatterns`, `files:getDefaultIgnorePatterns` near line 661)
- Preload: `src/preload/index.ts` (`watchFiles`, `unwatchFiles`, `updateFileIgnorePatterns`, `getDefaultFileIgnorePatterns`, `onFilesChanged`)
- Store: `src/renderer/src/lib/stores/fileTree.svelte.ts`
- Sidebar consumer: `src/renderer/src/components/sidebar/FileTreeSection.svelte`
- Diff consumer: `src/renderer/src/components/diff/DiffPane.svelte`
- Changes consumer: `src/renderer/src/components/diff/ChangesPanel.svelte`
- Editor consumer: `src/renderer/src/components/editor/EditorPane.svelte`
