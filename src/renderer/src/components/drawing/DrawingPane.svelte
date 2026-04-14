<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import {
    drawingsState,
    getDrawingKey,
    type Stroke,
    type DrawTool,
    type ShapeType,
  } from '../../lib/stores/drawings.svelte'
  import {
    strokeBBox,
    hitTest,
    redraw,
    canvasPoint,
    screenPoint,
    pointFromEvent,
  } from './drawingCanvas'
  import { deleteSelected, selectAll, undoLast, sendToAgent, copyPng } from './drawingActions'

  // Drawing pane has no props — canvas state is keyed by project via drawingsState.

  const COLORS = ['#e5e7eb', '#f87171', '#fbbf24', '#34d399', '#60a5fa', '#a78bfa']
  const SIZES = [3, 6, 12]

  let canvasEl: HTMLCanvasElement | undefined = $state()
  let containerEl: HTMLDivElement | undefined = $state()
  let dpr = 1
  let width = $state(0)
  let height = $state(0)

  let tool: DrawTool = $state('pen')
  let color = $state(COLORS[0])
  let size = $state(SIZES[1])
  let sending = $state(false)

  let currentKey: string | null = null
  let strokes: Stroke[] = $state([])
  let liveStroke: Stroke | null = $state(null)

  const selectedIds = new SvelteSet<string>()
  let marquee: { x0: number; y0: number; x1: number; y1: number } | null = $state(null)
  let panX = $state(0)
  let panY = $state(0)

  // Single combined effect: handle worktree/project switches AND mirror local strokes
  // into the persisted store. `untrack` on the writes prevents this effect from
  // re-triggering itself when it touches drawingsState (which it also reads on switch).
  $effect(() => {
    const k = getDrawingKey()
    const localStrokes = strokes
    untrack(() => {
      if (k !== currentKey) {
        if (currentKey) drawingsState[currentKey] = [...localStrokes]
        currentKey = k
        strokes = k ? [...(drawingsState[k] ?? [])] : []
        selectedIds.clear()
        panX = 0
        panY = 0
      } else if (currentKey) {
        drawingsState[currentKey] = [...localStrokes]
      }
    })
  })

  // --- Rendering ---

  function doRedraw(): void {
    if (!canvasEl || !containerEl) return
    redraw({
      canvas: canvasEl,
      container: containerEl,
      dpr,
      width,
      height,
      panX,
      panY,
      strokes,
      liveStroke,
      selectedIds,
      marquee,
    })
  }

  function resize(): void {
    if (!canvasEl || !containerEl) return
    const rect = containerEl.getBoundingClientRect()
    width = rect.width
    height = rect.height
    dpr = window.devicePixelRatio || 1
    canvasEl.width = Math.round(width * dpr)
    canvasEl.height = Math.round(height * dpr)
    canvasEl.style.width = `${width}px`
    canvasEl.style.height = `${height}px`
    doRedraw()
  }

  $effect(() => {
    void strokes.length
    void liveStroke
    void marquee
    void selectedIds.size
    void panX
    void panY
    doRedraw()
  })

  onMount(() => {
    resize()
    const ro = new ResizeObserver(resize)
    if (containerEl) ro.observe(containerEl)
    return () => {
      ro.disconnect()
    }
  })

  onDestroy(() => {
    if (currentKey) drawingsState[currentKey] = [...strokes]
  })

  // --- Pointer handling ---

  type DragMode = 'draw' | 'marquee' | 'click' | 'move' | 'pan' | null
  let dragMode: DragMode = null
  let activePointerId = -1
  let downPoint: { x: number; y: number } | null = null
  let lastDragPoint: { x: number; y: number } | null = null
  let isPanning = $state(false)

  const SHAPE_TOOLS: Record<string, ShapeType> = {
    rectangle: 'rectangle',
    ellipse: 'ellipse',
    line: 'line',
    arrow: 'arrow',
  }

  function isShapeTool(t: DrawTool): t is 'rectangle' | 'ellipse' | 'line' | 'arrow' {
    return t in SHAPE_TOOLS
  }

  let shapeOrigin: { x: number; y: number } | null = null

  function onPointerDown(e: PointerEvent): void {
    if (!canvasEl) return

    if (e.button === 1 && e.pointerType === 'mouse') {
      e.preventDefault()
      containerEl?.focus({ preventScroll: true })
      canvasEl.setPointerCapture(e.pointerId)
      activePointerId = e.pointerId
      dragMode = 'pan'
      isPanning = true
      lastDragPoint = screenPoint(e, canvasEl)
      return
    }

    if (e.button !== 0 && e.pointerType === 'mouse') return
    containerEl?.focus({ preventScroll: true })
    canvasEl.setPointerCapture(e.pointerId)
    activePointerId = e.pointerId
    const pt = canvasPoint(e, canvasEl, panX, panY)
    downPoint = pt
    lastDragPoint = pt

    if (tool === 'pen') {
      dragMode = 'draw'
      liveStroke = {
        id: crypto.randomUUID(),
        color,
        size,
        type: 'freehand',
        points: [pointFromEvent(e, canvasEl, panX, panY)],
      }
      return
    }

    if (isShapeTool(tool)) {
      dragMode = 'draw'
      const pt = canvasPoint(e, canvasEl, panX, panY)
      shapeOrigin = pt
      liveStroke = {
        id: crypto.randomUUID(),
        color,
        size,
        type: SHAPE_TOOLS[tool],
        rect: { x: pt.x, y: pt.y, w: 0, h: 0 },
      }
      return
    }

    const ctx = canvasEl.getContext('2d')
    if (!ctx) return
    const hit = hitTest(pt.x, pt.y, ctx, strokes, dpr)
    if (hit) {
      if (e.shiftKey) {
        if (selectedIds.has(hit.id)) selectedIds.delete(hit.id)
        else selectedIds.add(hit.id)
        dragMode = 'click'
      } else if (selectedIds.has(hit.id)) {
        dragMode = 'move'
      } else {
        selectedIds.clear()
        selectedIds.add(hit.id)
        dragMode = 'move'
      }
    } else {
      dragMode = 'marquee'
      if (!e.shiftKey) selectedIds.clear()
      const sp = screenPoint(e, canvasEl)
      marquee = { x0: sp.x, y0: sp.y, x1: sp.x, y1: sp.y }
    }
  }

  function onPointerMove(e: PointerEvent): void {
    if (e.pointerId !== activePointerId) return

    if (dragMode === 'draw' && liveStroke && canvasEl) {
      if (liveStroke.type === 'freehand') {
        liveStroke.points = [...liveStroke.points, pointFromEvent(e, canvasEl, panX, panY)]
      } else if (shapeOrigin && liveStroke.type !== 'freehand') {
        const pt = canvasPoint(e, canvasEl, panX, panY)
        let w = pt.x - shapeOrigin.x
        let h = pt.y - shapeOrigin.y
        if (e.shiftKey) {
          if (liveStroke.type === 'line' || liveStroke.type === 'arrow') {
            const angle = Math.atan2(h, w)
            const snapped = Math.round(angle / (Math.PI / 4)) * (Math.PI / 4)
            const len = Math.hypot(w, h)
            w = len * Math.cos(snapped)
            h = len * Math.sin(snapped)
          } else {
            const side = Math.max(Math.abs(w), Math.abs(h))
            w = side * Math.sign(w || 1)
            h = side * Math.sign(h || 1)
          }
        }
        liveStroke = { ...liveStroke, rect: { x: shapeOrigin.x, y: shapeOrigin.y, w, h } }
      }
      return
    }

    if (dragMode === 'marquee' && marquee && canvasEl) {
      const sp = screenPoint(e, canvasEl)
      marquee = { ...marquee, x1: sp.x, y1: sp.y }
      return
    }

    if (dragMode === 'move' && lastDragPoint && canvasEl) {
      const pt = canvasPoint(e, canvasEl, panX, panY)
      const dx = pt.x - lastDragPoint.x
      const dy = pt.y - lastDragPoint.y
      for (const s of strokes) {
        if (!selectedIds.has(s.id)) continue
        if (s.type !== 'freehand') {
          s.rect = { ...s.rect, x: s.rect.x + dx, y: s.rect.y + dy }
        } else {
          for (const p of s.points) {
            p[0] += dx
            p[1] += dy
          }
        }
      }
      strokes = [...strokes]
      lastDragPoint = pt
      return
    }

    if (dragMode === 'pan' && lastDragPoint && canvasEl) {
      const sp = screenPoint(e, canvasEl)
      panX += sp.x - lastDragPoint.x
      panY += sp.y - lastDragPoint.y
      lastDragPoint = sp
      return
    }
  }

  function onPointerUp(e: PointerEvent): void {
    if (e.pointerId !== activePointerId) return
    const mode = dragMode
    dragMode = null
    activePointerId = -1
    isPanning = false
    const down = downPoint
    downPoint = null
    lastDragPoint = null

    if (mode === 'draw' && liveStroke) {
      if (liveStroke.type === 'freehand') {
        if (liveStroke.points.length > 0) {
          strokes = [...strokes, liveStroke]
        }
      } else {
        const { w, h } = liveStroke.rect
        if (Math.abs(w) > 2 || Math.abs(h) > 2) {
          strokes = [...strokes, liveStroke]
        }
      }
      liveStroke = null
      shapeOrigin = null
      return
    }

    if (mode === 'marquee' && marquee && canvasEl) {
      const sp = screenPoint(e, canvasEl)
      const moved = Math.hypot(sp.x - marquee.x0, sp.y - marquee.y0) > 3
      if (moved) {
        const rx = Math.min(marquee.x0, marquee.x1) - panX
        const ry = Math.min(marquee.y0, marquee.y1) - panY
        const rw = Math.abs(marquee.x1 - marquee.x0)
        const rh = Math.abs(marquee.y1 - marquee.y0)
        for (const s of strokes) {
          const b = strokeBBox(s)
          const intersects = !(b.x + b.w < rx || b.x > rx + rw || b.y + b.h < ry || b.y > ry + rh)
          if (intersects) selectedIds.add(s.id)
        }
      }
      marquee = null
      return
    }

    if (mode === 'move' && down && canvasEl) {
      const pt = canvasPoint(e, canvasEl, panX, panY)
      const moved = Math.hypot(pt.x - down.x, pt.y - down.y) > 3
      if (!moved) {
        const ctx = canvasEl.getContext('2d')
        if (ctx) {
          const hit = hitTest(pt.x, pt.y, ctx, strokes, dpr)
          if (hit) {
            selectedIds.clear()
            selectedIds.add(hit.id)
          }
        }
      }
      return
    }
  }

  // --- Actions (delegated to drawingActions.ts) ---

  function handleDelete(): void {
    strokes = deleteSelected(strokes, selectedIds)
  }

  function handleUndo(): void {
    strokes = undoLast(strokes, selectedIds)
  }

  function handleClear(): void {
    if (strokes.length === 0) return
    strokes = []
    selectedIds.clear()
  }

  function onKeyDown(e: KeyboardEvent): void {
    const mod = e.metaKey || e.ctrlKey
    if (mod && e.key.toLowerCase() === 'z') {
      e.preventDefault()
      handleUndo()
      return
    }
    if (mod && e.key.toLowerCase() === 'a') {
      if (tool !== 'select') return
      e.preventDefault()
      selectAll(strokes, selectedIds)
      return
    }
    if ((e.key === 'Delete' || e.key === 'Backspace') && selectedIds.size > 0) {
      e.preventDefault()
      handleDelete()
      return
    }
    if (e.key === 'Escape' && selectedIds.size > 0) {
      e.preventDefault()
      selectedIds.clear()
    }
  }

  async function handleSendToAgent(): Promise<void> {
    if (sending || !canvasEl) return
    sending = true
    try {
      await sendToAgent(canvasEl, strokes, selectedIds, doRedraw)
    } finally {
      sending = false
    }
  }

  async function handleCopyPng(): Promise<void> {
    if (!canvasEl) return
    await copyPng(canvasEl, strokes, selectedIds, doRedraw)
  }
</script>

<div class="drawing-pane">
  <header class="drawing-header">
    <div class="tool-group" role="radiogroup" aria-label="Tool">
      <button
        type="button"
        role="radio"
        aria-checked={tool === 'pen'}
        class:active={tool === 'pen'}
        onclick={() => (tool = 'pen')}
        title="Pen"
      >
        Pen
      </button>
      <button
        type="button"
        role="radio"
        aria-checked={tool === 'select'}
        class:active={tool === 'select'}
        onclick={() => (tool = 'select')}
        title="Select (drag to marquee, Shift+click to toggle, Del to delete)"
      >
        Select
      </button>
      <span class="tool-sep"></span>
      <button
        type="button"
        role="radio"
        aria-checked={tool === 'rectangle'}
        class:active={tool === 'rectangle'}
        onclick={() => (tool = 'rectangle')}
        title="Rectangle (Shift for square)"
      >
        Rect
      </button>
      <button
        type="button"
        role="radio"
        aria-checked={tool === 'ellipse'}
        class:active={tool === 'ellipse'}
        onclick={() => (tool = 'ellipse')}
        title="Ellipse (Shift for circle)"
      >
        Ellipse
      </button>
      <button
        type="button"
        role="radio"
        aria-checked={tool === 'line'}
        class:active={tool === 'line'}
        onclick={() => (tool = 'line')}
        title="Line (Shift to snap angle)"
      >
        Line
      </button>
      <button
        type="button"
        role="radio"
        aria-checked={tool === 'arrow'}
        class:active={tool === 'arrow'}
        onclick={() => (tool = 'arrow')}
        title="Arrow (Shift to snap angle)"
      >
        Arrow
      </button>
    </div>

    <div class="swatches" role="radiogroup" aria-label="Color">
      {#each COLORS as c (c)}
        <button
          type="button"
          role="radio"
          aria-checked={color === c}
          class:active={color === c}
          class="swatch"
          style:background={c}
          onclick={() => (color = c)}
          aria-label={`Color ${c}`}
        ></button>
      {/each}
    </div>

    <div class="size-group" role="radiogroup" aria-label="Stroke size">
      {#each SIZES as s (s)}
        <button
          type="button"
          role="radio"
          aria-checked={size === s}
          class:active={size === s}
          onclick={() => (size = s)}
          aria-label={`Size ${s}`}
        >
          <span class="dot" style:width="{s}px" style:height="{s}px"></span>
        </button>
      {/each}
    </div>

    <div class="spacer"></div>

    {#if selectedIds.size > 0}
      <button type="button" class="danger" onclick={handleDelete} title="Delete (Del)">
        Delete ({selectedIds.size})
      </button>
    {/if}
    <button type="button" onclick={handleUndo} disabled={strokes.length === 0} title="Undo (⌘Z)">
      Undo
    </button>
    <button type="button" class="danger" onclick={handleClear} disabled={strokes.length === 0}>
      Clear
    </button>
    <button type="button" onclick={handleCopyPng} disabled={strokes.length === 0}>Copy PNG</button>
    <button
      type="button"
      class="primary"
      onclick={handleSendToAgent}
      disabled={sending || strokes.length === 0}
    >
      {sending ? 'Sending…' : 'Send to agent'}
    </button>
  </header>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="canvas-wrap"
    bind:this={containerEl}
    tabindex="0"
    role="application"
    aria-label="Drawing canvas"
    onkeydown={onKeyDown}
    onauxclick={(e) => {
      if (e.button === 1) e.preventDefault()
    }}
  >
    <canvas
      bind:this={canvasEl}
      class:select-cursor={tool === 'select'}
      class:crosshair-cursor={isShapeTool(tool)}
      class:panning={isPanning}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      onlostpointercapture={onPointerUp}
    ></canvas>
  </div>
</div>

<style>
  .drawing-pane {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--c-bg);
    color: var(--c-text);
  }

  .drawing-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-bg-elevated);
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .drawing-header button {
    background: transparent;
    border: 1px solid var(--c-border);
    color: var(--c-text-secondary);
    padding: 4px 10px;
    font-size: 12px;
    border-radius: 4px;
    cursor: pointer;
  }

  .drawing-header button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .drawing-header button:not(:disabled):hover {
    color: var(--c-text);
  }

  .tool-group,
  .size-group,
  .swatches {
    display: inline-flex;
    gap: 4px;
    align-items: center;
  }

  .drawing-header .active {
    background: var(--c-accent);
    color: var(--c-bg);
    border-color: var(--c-accent);
  }

  .drawing-header button.swatch {
    width: 22px;
    height: 22px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid var(--c-border);
    box-sizing: border-box;
    flex-shrink: 0;
  }

  .drawing-header button.swatch.active {
    background: inherit;
    border-color: transparent;
    box-shadow: 0 0 0 2px var(--c-text);
  }

  .size-group button {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .dot {
    background: currentColor;
    border-radius: 50%;
    display: inline-block;
  }

  .spacer {
    flex: 1;
  }

  .drawing-header button.primary:not(:disabled) {
    background: var(--c-accent);
    color: var(--c-bg);
    border-color: var(--c-accent);
  }

  .drawing-header button.danger:not(:disabled):hover {
    color: var(--c-danger-text);
    border-color: var(--c-danger);
  }

  .canvas-wrap {
    flex: 1;
    min-height: 0;
    position: relative;
    overflow: hidden;
    outline: none;
  }

  canvas {
    display: block;
    touch-action: none;
    cursor: crosshair;
  }

  canvas.select-cursor {
    cursor: default;
  }

  canvas.crosshair-cursor {
    cursor: crosshair;
  }

  .tool-sep {
    width: 1px;
    height: 18px;
    background: var(--c-border);
    margin: 0 2px;
  }

  canvas.panning {
    cursor: grabbing;
  }
</style>
