# Sidebar tools

> Reorder the sidebar TOOLS list and hide individual tools, without removing them from the command palette.

**Status:** Stable
**Introduced:** 0.13.0
**Platforms:** macOS, Windows, Linux

## Overview

The sidebar TOOLS section lists every registered tool — built-in AI agents (Claude, Gemini, OpenCode, Codex) and user-registered custom CLI tools — in a single ordered list. The **Tools** section of Settings lets the user choose that order and hide tools they rarely launch.

A hidden tool is removed only from the sidebar. It stays fully registered and remains launchable from the command palette, so hiding is a decluttering action, not an uninstall.

The order and per-tool visibility are stored together in a single `tools.view` preference. The default view — used when the preference is empty or missing — shows every tool as visible, in the tool registry's own order, so a fresh install and any user who never opens the Tools settings see the unchanged sidebar.

## Behavior

### Reorder a tool

1. User opens Settings → **Tools**.
2. Each row has **Move up** / **Move down** arrow buttons. The up arrow is disabled on the first row; the down arrow is disabled on the last row.
3. Clicking an arrow swaps the tool with its neighbor and persists the new order to `tools.view` immediately.
4. Keyboard focus stays on the moved row: focus follows the arrow that was pressed to its new position, and if that arrow becomes disabled at the top/bottom boundary, focus falls back to the opposite arrow on the same row so repeated keyboard reordering never drops focus to `<body>`.
5. The sidebar TOOLS section reflects the new order on the next reactive read.

### Hide / show a tool

1. In Settings → **Tools**, each row has an eye toggle (`Eye` when visible, `EyeOff` when hidden).
2. Clicking it flips the tool's `visible` flag and persists `tools.view`.
3. Hidden rows are dimmed in the settings list but stay editable and reorderable.
4. In the sidebar, `ToolSection` renders only tools that are **visible** _and_ currently **available** (installed / resolvable) — an unavailable tool is omitted even when marked visible.
5. Hidden tools remain in the command palette and can still be launched from there.

### Reconciliation with the tool set

`getToolView()` reconciles the saved config against the live tool registry on every read:

- Saved entries whose `id` still exists in the registry keep their saved order and visibility.
- Tools present in the registry but absent from the saved config (newly installed built-ins, freshly added custom tools) are appended to the end as **visible**.
- Saved entries whose `id` no longer exists in the registry (removed custom tools) are dropped.
- Malformed saved JSON, or a non-array payload, is treated as an empty config, so a corrupt value degrades to the all-visible default rather than throwing.

## Configuration

| Key          | Type                                     | Default                                   | Description                                                    |
| ------------ | ---------------------------------------- | ----------------------------------------- | -------------------------------------------------------------- |
| `tools.view` | JSON string of `{ id, visible }[]` array | empty → all tools visible, registry order | Ordered sidebar view: per-tool position and sidebar visibility |

The preference is written as a JSON-serialized array via the renderer preferences store (`setPref`) and read back with `getPref`. It has no dedicated settings toggle beyond the reorder arrows and eye toggles in the Tools section; there is no way to reach an invalid state from the UI.

## Source files

- Store / reconciliation: `src/renderer/src/lib/stores/toolView.svelte.ts` (`getToolView`, `toggleToolVisibility`, `moveToolUp`, `moveToolDown`)
- Settings UI: `src/renderer/src/components/preferences/ToolPrefs.svelte`
- Sidebar consumer: `src/renderer/src/components/sidebar/ToolSection.svelte`
- Preferences bridge: `src/renderer/src/lib/stores/preferences.svelte` (`getPref`, `setPref`)
