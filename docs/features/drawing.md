# Drawing

> Freehand sketchpad with pressure-aware strokes — send drawings to an active AI agent.

**Status:** Stable
**Introduced:** v0.11.0
**Platforms:** macOS, Windows, Linux

## Overview

The Drawing pane provides a canvas-based sketchpad for quick visual communication with AI agents. Users draw with a pen tool, select and delete strokes, then send the result as a PNG image to an active Claude or Codex pane via the system clipboard.

Drawings are stored in renderer memory keyed by project (repo root). They are ephemeral — closing all drawing panes clears the drawing state, and nothing is persisted to disk. The canvas uses the `perfect-freehand` library for smooth, pressure-aware stroke rendering and reads theme CSS variables for background and selection colors.

## Behavior

### Drawing

1. With the **Pen** tool active, the user presses down on the canvas.
2. A live stroke is created with the current color and size. Pointer move events append `[x, y, pressure]` points.
3. On pointer up, the live stroke is committed to the strokes array.
4. Each frame, `redraw()` clears the canvas, fills the theme background, and renders all committed strokes plus the in-progress live stroke using `perfect-freehand` outlines.

### Shapes

1. With a shape tool active (**Rect**, **Ellipse**, **Line**, or **Arrow**), the user clicks and drags on the canvas.
2. The origin point is the pointer-down position; the current pointer position defines the shape extent.
3. A live preview of the shape renders during the drag.
4. On pointer up, shapes smaller than 2px in both dimensions are discarded.
5. **Shift** constrains: rectangles to squares, ellipses to circles, lines/arrows to 45° angle increments.
6. Shapes use the current color and stroke size. They are outline-only (no fill).
7. Shapes participate in all selection, move, delete, and undo operations identically to freehand strokes.

### Selection

1. With the **Select** tool active, clicking a stroke selects it (highlighted with an accent-colored outline). **Shift+click** toggles individual strokes.
2. Dragging on empty space draws a marquee rectangle. On release, all strokes whose bounding boxes intersect the marquee are selected. **Shift+drag** adds to the existing selection.
3. Hit testing uses `CanvasRenderingContext2D.isPointInPath()` against stroke outlines, iterating top-to-bottom (last drawn first).
4. **Moving strokes:** Clicking and dragging a selected stroke repositions all selected strokes. A drag shorter than 3px is treated as a click-to-reselect instead.

### Canvas pan

Middle mouse button pans the canvas viewport regardless of the active tool. Strokes are stored in world-space coordinates; the pan offset is applied at render time via `ctx.translate()`. Pan resets to (0, 0) on worktree/project switch.

### Actions

| Action          | Trigger                                           | Effect                               |
| --------------- | ------------------------------------------------- | ------------------------------------ |
| Delete selected | **Delete** / **Backspace** key, or toolbar button | Removes selected strokes             |
| Undo            | **Cmd+Z**                                         | Removes the last committed stroke    |
| Select all      | **Cmd+A** (select mode)                           | Selects all strokes                  |
| Deselect        | **Escape**                                        | Clears the selection                 |
| Move selected   | Drag a selected stroke (select mode)              | Repositions all selected strokes     |
| Pan canvas      | **Middle mouse button** drag                      | Scrolls the canvas viewport          |
| Draw shape      | Click-drag with shape tool, Shift for constrain   | Creates rectangle/ellipse/line/arrow |
| Clear           | Toolbar button                                    | Removes all strokes                  |

### Exporting

1. **Copy PNG** converts the canvas to a PNG blob via `canvas.toBlob()` and writes it to the system clipboard.
2. If strokes are selected, the selection highlight is temporarily hidden before export, then restored.

### Sending to an agent

1. User clicks **Send to agent**.
2. The drawing is exported as PNG and written to the clipboard.
3. The active tab switches to the agent's tab if it is in a different tab.
4. After a 250ms delay (macOS pasteboard race), `Ctrl+V` is injected into the active agent PTY via `window.api.writePty()`.
5. Agent pane detection searches in priority order: focused pane in active tab, any running agent in active tab, any running agent in other tabs.
6. A toast confirms success or reports errors (no agent pane, pane not running, clipboard failure).

### Worktree/project switching

1. Drawings are keyed by project context (`repoRoot`, then `workspace.path`, then `worktreePath`).
2. When the user switches worktrees, the `$effect` saves current strokes to `drawingsState[oldKey]` and loads strokes for the new key.
3. The selection is cleared on switch.

### Cleanup

1. When the last drawing pane is closed, all entries in `drawingsState` are deleted.
2. Drawing panes are never serialized to the layout store.

## Tools and options

**Tools:** Pen (freehand drawing), Select (click/marquee/shift-multi-select), Rectangle, Ellipse, Line, Arrow

**Colors:** 6 swatches — light gray (`#e5e7eb`), red (`#f87171`), amber (`#fbbf24`), emerald (`#34d399`), blue (`#60a5fa`), purple (`#a78bfa`)

**Stroke sizes:** 3px, 6px (default), 12px

## Theming

Canvas rendering reads CSS custom properties from the container element:

| CSS variable    | Usage                      |
| --------------- | -------------------------- |
| `--c-bg`        | Canvas background fill     |
| `--c-accent`    | Selection highlight stroke |
| `--c-accent-bg` | Marquee selection fill     |

If the container has not yet received theme styles, `getThemeColor` falls back to `document.documentElement` computed styles before using hardcoded defaults.

## Source files

- Component: `src/renderer/src/components/drawing/DrawingPane.svelte`
- Canvas rendering & hit testing: `src/renderer/src/components/drawing/drawingCanvas.ts`
- Actions & export logic: `src/renderer/src/components/drawing/drawingActions.ts`
- Store: `src/renderer/src/lib/stores/drawings.svelte.ts`
- Pane routing: `src/renderer/src/components/terminal/PaneWrapper.svelte`
- Command palette entry: `src/renderer/src/components/palette/CommandPalette.svelte`
