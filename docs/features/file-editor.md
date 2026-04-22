# File Editor

> Built-in CodeMirror 6 editor for viewing and editing workspace files, with
> multi-file sub-tabs per pane.

**Status:** Stable
**Introduced:** v0.12.0-next
**Platforms:** macOS, Windows, Linux

## Overview

Clicking a file in the sidebar file tree opens it in an editor pane. The pane
is a `paneType: 'editor'` that participates in the standard split/drag/close
system but renders CodeMirror instead of a terminal.

Each editor pane holds a list of open files as **sub-tabs** (similar to VS
Code's editor groups). Opening additional files reuses the active editor pane
rather than creating new top-level tabs. Sub-tabs can be reordered by drag,
detached to a new tab by dragging them out of the strip, or merged into
another editor pane by dragging and dropping.

Syntax highlighting, indentation, and theme colors follow the workspace
terminal theme so the editor looks like the rest of Canopy. All editor chrome
(gutters, cursor, selection, syntax tokens) is driven by CSS custom
properties derived from the xterm ANSI palette.

## Behavior

### Opening a file

1. The user clicks a file entry in the sidebar file tree.
   `openFile(filePath, worktreePath)` is invoked.
2. If the file is already open anywhere, the existing sub-tab is focused.
3. Else, if the active tab in this worktree already has an editor pane, the
   file is appended as a sub-tab and made active.
4. Otherwise, a new top-level tab titled "Editor" is created with the file
   as its only sub-tab.

### Loading content

1. `EditorPane` calls `window.api.readFile(filePath, 2 MB)` and
   `window.api.statFile(filePath)` in sequence.
2. Files above 2 MB or with null bytes in the first 8 KB are treated as
   read-only (binary / truncated). A banner explains why.
3. If the user has no write permission (`statFile.canWrite === false`), the
   editor is shown read-only with a banner.
4. The file's line ending is detected (LF vs CRLF) and preserved on save.
5. `detectIndent` inspects up to 1000 leading-whitespace samples and picks
   the indent unit (tabs or N spaces, snapped to {2, 4, 8}) based on the
   most common positive step between consecutive indent levels. Users can
   override via the status bar dropdowns.

### Editing and saving

1. CodeMirror emits every doc change to `onChange`, which marks the active
   sub-tab dirty if the content differs from disk.
2. A dirty sub-tab shows a gray dot; the hosting top-level tab also shows a
   gray dot if any of its editor sub-tabs are dirty.
3. `Cmd/Ctrl+S` (or clicking the Save button) writes the buffer via
   `window.api.writeFile`. Line endings are normalized to match the file's
   original style.
4. Right before writing, `skipNextWatcherEvent = true` is set. When the
   parcel file-tree watcher reports the self-generated change event within
   1500 ms, the editor skips the reload — preserving cursor position and
   undo history.

### Stale-write detection

1. `writeFile` includes the last-known `mtimeMs` as `expectedMtimeMs`.
2. If the main process detects the on-disk mtime has changed since load,
   the write is rejected with `StaleWrite` and the editor shows the
   conflict banner.

### External change detection

1. The shared `onFilesChanged` stream (backed by `@parcel/watcher`) notifies
   the editor when any file changes on disk.
2. If the buffer is **clean**, the file is silently reloaded (no cursor
   disruption because CodeMirror's doc replacement only happens at load).
3. If the buffer is **dirty**, a conflict banner appears with
   **[Reload (discard changes)]** and **[Keep mine]**. Until the user
   chooses, the editor keeps the in-memory buffer.

### Sub-tab operations

| Action                           | How                                                                        |
| -------------------------------- | -------------------------------------------------------------------------- |
| Switch to sub-tab                | Left-click.                                                                |
| Close sub-tab                    | Middle-click, or click the × icon. Dirty files prompt Save/Discard/Cancel. |
| Reorder sub-tabs                 | Drag within the strip. Blue drop indicator shows insertion point.          |
| Move sub-tab to another editor   | Drag into another pane's strip.                                            |
| Detach sub-tab to a new top tab  | Drag it out of any strip, or right-click → context detach.                 |
| Merge a whole tab into the strip | Drag the top-level tab from the tab bar onto the strip.                    |

When a cross-pane move or detach leaves the source pane with zero files,
the pane is collapsed (and the top-level tab closes if it was the only
pane).

### Closing a tab with dirty sub-tabs

`closeTab` gathers all dirty editor files across every editor pane in the
tab and shows a native dialog:

- **Save** — writes every dirty file sequentially; if any write fails,
  close is aborted.
- **Don't Save** — discards changes and closes.
- **Cancel** — leaves everything open.

### Status bar

The status bar below the editor shows:

- The detected line ending (`LF` or `CRLF`).
- **Indent:** Spaces / Tabs toggle.
- **Size:** 2 / 4 / 8 (spaces only).

Changing any option applies immediately via CodeMirror's `indentUnit`
compartment — without recreating the view, so cursor and undo history are
preserved.

## Syntax highlighting and theme

- Language mode is selected by filename via `@codemirror/language-data`
  (lazy-loaded per language).
- A single theme extension maps CodeMirror tokens onto CSS custom
  properties: `--c-syntax-keyword`, `--c-syntax-string`,
  `--c-syntax-comment`, `--c-syntax-number`, `--c-syntax-operator`,
  `--c-syntax-function`, `--c-syntax-variable`, `--c-syntax-property`,
  `--c-syntax-type`, `--c-syntax-tag`, `--c-syntax-attribute`,
  `--c-syntax-heading`, `--c-syntax-invalid`.
- These vars are derived in `deriveAppTheme()` from the xterm ANSI colors
  (magenta, green, yellow, cyan, blue) so a theme switch reflows editor
  colors without reinitialization.
- Indentation guides use `--c-indent-guide` and `--c-indent-guide-active`.

## Clickable paths

Terminal output (xterm link provider) and AI tool output
(`LinkifiedText.svelte`) use the shared `detectPathsInText` regex to
turn path-looking tokens into clickable links. Clicking a link calls
`openFile(absolutePath, worktreePath, { line })` — the editor opens the
file and jumps to the line via the exposed `goToLine` on the component.

## Persistence

Per-pane editor state that **is** persisted across restarts:

- List of open sub-tab paths (`editorFiles`).
- Active sub-tab path (`editorActiveFile`).

Per-pane editor state that **is not** persisted (intentional):

- Dirty flag, unsaved buffer content, mtime, line ending — the editor
  reloads each sub-tab from disk on restore.
- Scroll position and cursor selection.

A pane with zero files after a merge/detach is not restored.

## IPC surface

| Handler                        | Purpose                                                                    |
| ------------------------------ | -------------------------------------------------------------------------- |
| `fs:readFile`                  | Bounded read (up to 10 MB hard cap), binary detection, truncation flag.    |
| `fs:writeFile`                 | Writes UTF-8, optionally gated by `expectedMtimeMs` to detect stale saves. |
| `fs:stat`                      | Returns `{ mtimeMs, size, canWrite }`.                                     |
| `dialog:confirmUnsavedChanges` | Native 3-way dialog (Save / Don't Save / Cancel) used by `closeTab`.       |

All three validate `filePath` through `validatePathAccess(senderId, path)`
which resolves symlinks and rejects anything outside the renderer's
workspace roots.

## Keyboard shortcuts

| Shortcut                        | Action                                        |
| ------------------------------- | --------------------------------------------- |
| **Cmd/Ctrl+S**                  | Save the active file.                         |
| **Cmd/Ctrl+F**                  | Open CodeMirror find widget in the editor.    |
| **Cmd/Ctrl+H**                  | Open find-and-replace widget in the editor.   |
| **Cmd/Ctrl+Z**/ **⇧Cmd/Ctrl+Z** | Undo / redo.                                  |
| **Ctrl+Space**                  | Force autocomplete dropdown (keyword buffer). |
| **Tab** / **⇧Tab**              | Indent / dedent (works on selection).         |
| **Middle-click on sub-tab**     | Close the sub-tab.                            |

## Security / privacy

- All reads and writes go through the preload bridge; the renderer has no
  direct `fs` access.
- `validatePathAccess` uses `fs.realpath` to defeat symlink escapes before
  any read or write, including stat.
- The editor refuses to mount CodeMirror for files whose size exceeds
  2 MB or whose first 8 KB contain NUL bytes (read-only fallback).
- No content is transmitted off-machine; files stay in renderer memory
  until explicitly saved to disk.

## Source files

- Component: `src/renderer/src/components/editor/EditorPane.svelte`
- CodeMirror wrapper: `src/renderer/src/components/editor/CodeMirrorEditor.svelte`
- Extensions / theme / language: `src/renderer/src/components/editor/cm/`
- Indent detection: `src/renderer/src/components/editor/cm/detectIndent.ts`
- State + sub-tab operations: `src/renderer/src/lib/stores/tabs.svelte.ts`
- Pane routing: `src/renderer/src/components/terminal/PaneWrapper.svelte`
- IPC handlers: `src/main/ipc/handlers.ts` (`fs:readFile`, `fs:writeFile`, `fs:stat`, `dialog:confirmUnsavedChanges`)
- IPC error type: `src/main/ipc/fsErrors.ts`
- Path detection: `src/renderer/src/lib/pathDetection/linkify.ts`
- Theme CSS vars: `src/renderer/src/lib/theme/appTheme.ts`
