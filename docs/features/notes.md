# Notes

> In-memory markdown scratch pad scoped to a worktree or project.

**Status:** Stable
**Introduced:** v0.11.0
**Platforms:** macOS, Windows, Linux

## Overview

The Notes pane provides a lightweight markdown editor for jotting down context while working. Notes are ephemeral — they live in renderer memory only and are not persisted to disk. Closing the app discards all notes.

Each Notes pane supports two scoping modes: **worktree** (branch-specific) and **project** (shared across all branches in the same repo). Switching scopes swaps the content to the corresponding storage slot. A live markdown preview is available as a toggleable split view.

Notes panes are non-terminal panes — they have no associated process, PTY, or IPC. They participate in the standard pane system (split, drag, close) but are explicitly excluded from layout persistence.

## Behavior

### Opening a notes pane

1. User opens the command palette (**Cmd+K**) and selects **Open Notes**.
2. `openTool('notes', worktreePath)` creates a new `PaneSession` with `paneType: 'notes'` in the active tab.
3. The pane loads the stored note content for the current worktree key (or an empty string if none exists).

### Editing

1. The user types in the textarea. Each input event writes the content into `notesState[key]` immediately.
2. A debounced `$effect` parses the markdown into HTML via `marked` and sanitizes it with `DOMPurify`.
3. If the preview panel is visible, the rendered HTML updates after the debounce period.

### Scope switching

1. The header displays two toggle buttons: **Worktree** and **Project**.
2. Clicking a scope button updates `notesUiScope[paneSessionId]` to the chosen scope.
3. The note key changes: worktree scope uses `worktreePath`, project scope uses `repoRoot` (or `workspace.path`).
4. Content swaps to the note stored under the new key. Both scopes retain their content independently.

### Preview

1. Clicking **Show preview** splits the pane horizontally into editor and rendered HTML.
2. Clicking **Hide preview** collapses back to editor-only.
3. The preview panel uses a monospace font with styled headings, code blocks, and links.

### Worktree/project switching

1. When the user selects a different worktree, the `$effect` watching `getNoteKey()` detects the key change.
2. The current note content is saved to `notesState[oldKey]`.
3. Content for the new key is loaded (or initialized as empty).

### Cleanup

1. Closing a Notes pane calls `disposeEphemeralPaneState()`, which deletes the per-pane UI scope entry.
2. Note content in `notesState` is retained as long as the app is running, so reopening a Notes pane for the same worktree/project recovers the content.
3. Notes panes are never serialized to the layout store — they are excluded during persistence.

## Keyboard shortcuts

| Shortcut                    | Action                                    |
| --------------------------- | ----------------------------------------- |
| **Cmd+K** → "Open Notes"    | Open a new Notes pane via command palette |
| **Cmd+W**                   | Close the focused pane (including Notes)  |
| **Cmd+D** / **Cmd+Shift+D** | Split the active pane                     |

Standard text editing shortcuts (Cmd+Z, Cmd+A, etc.) work natively in the textarea.

## Source files

- Component: `src/renderer/src/components/notes/NotesPane.svelte`
- Store: `src/renderer/src/lib/stores/notes.svelte.ts`
- Pane routing: `src/renderer/src/components/terminal/PaneWrapper.svelte`
- Command palette entry: `src/renderer/src/components/palette/CommandPalette.svelte`
