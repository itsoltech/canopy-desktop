# Welcome dashboard

> Picker shown when no project is open, listing recent workspaces and offering ways to open a new one.

**Status:** Stable
**Introduced:** v0.13.0 (current redesign)
**Platforms:** All

## Overview

The welcome dashboard is the empty-state surface rendered by `MainLayout` when no projects are attached and no tabs are open. It serves two roles: a fast picker for returning users (filter + keyboard navigation over recent workspaces) and a clear entry point for new users or after detaching the last project (dedicated empty state with an auto-focused Open Folder action).

Recent workspaces are sourced from the workspace database. The first three git repos are refreshed in the background after mount so branch, dirty status, ahead/behind, and worktree count reflect current disk state without blocking the initial render.

## Behavior

### Populated state

Rendered when one or more workspaces are present in the database.

1. Top row shows a small "CANOPY" wordmark and shortcut hints (`⌘O` / `Ctrl O` open, `/` filter when filter is shown).
2. Filter input appears only when more than 4 recents exist, mirroring the search affordance from app preferences. Matches against workspace name, path, and cached branch (case-insensitive substring).
3. The "Recent" section header shows a count: `N` total, or `N / total` while filtering.
4. Each row shows: name, branch, dirty marker (`*`), ahead/behind indicators (`↑`/`↓`), absolute path (mono, truncated with full path in `title` tooltip), and either worktree count or relative-time stamp.
5. Footer holds Open Folder and Open from Path actions plus a keyboard hint row.

### Empty state

Rendered when no workspaces exist (fresh install or after detaching the last project).

1. Large Canopy wordmark and one-line tagline.
2. Primary `Open Folder` button auto-focused on mount, with `⌘O` / `Ctrl O` kbd hint embedded in the button.
3. Secondary `Open from Path…` text-button below.

### Keyboard navigation

The dashboard uses a roving-tabindex listbox pattern. On mount with recents, the first row is focused so the user can start navigating immediately.

| Key                     | Action                                                                                 |
| ----------------------- | -------------------------------------------------------------------------------------- |
| `↑` / `↓`               | Move selection (focus follows when on a row).                                          |
| `PageUp` / `PageDown`   | Jump 5 rows.                                                                           |
| `Home` / `End`          | Jump to first / last (only when on a row, not while typing in filter).                 |
| `Enter`                 | Open the selected workspace (works from both filter and row).                          |
| `/`                     | Focus and select the filter input (only when filter is shown).                         |
| Any printable character | When focused on a row and filter is empty, switches focus to filter and starts typing. |
| `Esc`                   | Clears the filter when non-empty; otherwise moves focus from filter back to the list.  |
| `⌘⌫` / `Ctrl+Backspace` | Remove the selected workspace from recents (with confirm dialog).                      |

Navigation keys are scoped to the filter input and row buttons, so `Enter` on Open Folder / Open from Path activates those buttons normally instead of opening a workspace.

### Recent-workspace context menu

Right-clicking a row opens a small context menu with three actions:

- **Show in Finder / Show in File Manager** — platform-resolved label via `fileManagerLabel()`; calls `window.api.showInFolder(path)`.
- **Copy Path** — writes the absolute path to the clipboard.
- **Remove from Recent** — confirmation-dialog-gated removal via `window.api.removeWorkspace(id)`.

`Escape` closes the menu.

### Background git refresh

After the initial DB query, the first 3 git workspaces are refreshed sequentially via `window.api.refreshWorkspaceGitStatus(id, path)`. Failures (path moved/deleted) are swallowed silently — the dashboard still renders the cached row. The refresh loop checks a `disposed` flag so it stops cleanly if the dashboard unmounts (project attached) before it finishes.

### Returning to the dashboard

`MainLayout` renders the dashboard only when `projects.length === 0 && allTabs.length === 0`. When the user detaches the last project (`detachProject` in `workspace.svelte.ts`), every owned tab key is removed from `tabsByWorktree` — including stale or symlink-normalized keys under the repo root — and a `killAllTabs()` safety net runs if any entries remain. This guarantees `allTabs` is empty so the dashboard re-mounts.

## Configuration

No user-facing preferences. Recent count is hard-coded to 20 entries on initial load; the filter affordance is hard-gated to `workspaces.length > 4`.

## Error states

- `listWorkspaces` failure: dashboard shows the empty state (it relies on the resolved array being empty rather than a thrown error path; the IPC handler returns `[]` on failure).
- `refreshWorkspaceGitStatus` failure for a recent row: silently caught — the row keeps its cached values.
- `removeWorkspace` IPC failure: the row is still removed from the local list because the call is awaited only for the side-effect; subsequent reloads will reconcile.

## Security / Privacy

Recent paths are stored locally in the workspace SQLite database and never transmitted. `Copy Path` writes to the system clipboard only on explicit user click. The `prompt()` dialog used by Open from Path does not validate filesystem access; downstream `openWorkspace()` is responsible for handling missing or unreadable directories.

## Source files

- `src/renderer/src/components/dashboard/WelcomeDashboard.svelte` — orchestration: data fetch, keyboard handler, context menu state.
- `src/renderer/src/components/dashboard/_partials/WelcomeEmpty.svelte` — empty state hero.
- `src/renderer/src/components/dashboard/_partials/WelcomeRecents.svelte` — populated state (header, filter input, list, footer).
- `src/renderer/src/components/dashboard/_partials/RecentRow.svelte` — single row markup with branch/path/metadata.
- `src/renderer/src/components/dashboard/_partials/WelcomeContextMenu.svelte` — right-click menu.
- `src/renderer/src/components/layout/MainLayout.svelte` — mount site (`projects.length === 0 && allTabs.length === 0`).
- `src/renderer/src/lib/stores/workspace.svelte.ts` — `detachProject` cleanup that ensures the dashboard re-mounts after the last project is removed.
- `src/preload/index.ts` — IPC surface used: `listWorkspaces`, `refreshWorkspaceGitStatus`, `removeWorkspace`, `openFolder`, `showInFolder`.
