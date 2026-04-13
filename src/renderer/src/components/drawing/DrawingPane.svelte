<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { getStroke } from 'perfect-freehand'
  import {
    drawingsState,
    getDrawingKey,
    type Stroke,
    type StrokePoint,
    type DrawTool,
  } from '../../lib/stores/drawings.svelte'
  import { getActiveAgentPane } from '../../lib/stores/tabs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'

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
      } else if (currentKey) {
        drawingsState[currentKey] = [...localStrokes]
      }
    })
  })

  // --- Stroke geometry ---

  function strokePath(stroke: Stroke): Path2D {
    const path = new Path2D()
    const outline = getStroke(stroke.points, {
      size: stroke.size,
      thinning: 0.55,
      smoothing: 0.55,
      streamline: 0.5,
      simulatePressure: true,
    })
    if (outline.length === 0) return path
    path.moveTo(outline[0][0], outline[0][1])
    for (let i = 1; i < outline.length; i++) {
      path.lineTo(outline[i][0], outline[i][1])
    }
    path.closePath()
    return path
  }

  function strokeBBox(stroke: Stroke): { x: number; y: number; w: number; h: number } {
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity
    for (const [x, y] of stroke.points) {
      if (x < minX) minX = x
      if (y < minY) minY = y
      if (x > maxX) maxX = x
      if (y > maxY) maxY = y
    }
    const pad = stroke.size
    return { x: minX - pad, y: minY - pad, w: maxX - minX + 2 * pad, h: maxY - minY + 2 * pad }
  }

  function hitTest(px: number, py: number): Stroke | null {
    const canvas = canvasEl
    if (!canvas) return null
    const ctx = canvas.getContext('2d')
    if (!ctx) return null
    // Last drawn stroke sits on top — iterate back-to-front.
    for (let i = strokes.length - 1; i >= 0; i--) {
      if (ctx.isPointInPath(strokePath(strokes[i]), px * dpr, py * dpr)) {
        return strokes[i]
      }
    }
    return null
  }

  // --- Rendering ---

  function getThemeColor(prop: string, fallback: string): string {
    if (!containerEl) return fallback
    return getComputedStyle(containerEl).getPropertyValue(prop).trim() || fallback
  }

  function redraw(): void {
    const canvas = canvasEl
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const bg = getThemeColor('--c-bg', '#1e1e1e')
    const accent = getThemeColor('--c-accent', '#60a5fa')
    const accentBg = getThemeColor('--c-accent-bg', 'rgba(96, 165, 250, 0.12)')

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.fillStyle = bg
    ctx.fillRect(0, 0, width, height)

    for (const s of strokes) {
      const path = strokePath(s)
      ctx.fillStyle = s.color
      ctx.fill(path)
      if (selectedIds.has(s.id)) {
        ctx.save()
        ctx.strokeStyle = accent
        ctx.lineWidth = 2
        ctx.stroke(path)
        ctx.restore()
      }
    }
    if (liveStroke) {
      ctx.fillStyle = liveStroke.color
      ctx.fill(strokePath(liveStroke))
    }
    if (marquee) {
      ctx.save()
      const x = Math.min(marquee.x0, marquee.x1)
      const y = Math.min(marquee.y0, marquee.y1)
      const w = Math.abs(marquee.x1 - marquee.x0)
      const h = Math.abs(marquee.y1 - marquee.y0)
      ctx.fillStyle = accentBg
      ctx.strokeStyle = accent
      ctx.lineWidth = 1
      ctx.fillRect(x, y, w, h)
      ctx.strokeRect(x + 0.5, y + 0.5, w, h)
      ctx.restore()
    }
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
    redraw()
  }

  $effect(() => {
    void strokes.length
    void liveStroke
    void marquee
    void selectedIds.size
    redraw()
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

  type DragMode = 'draw' | 'marquee' | 'click' | null
  let dragMode: DragMode = null
  let activePointerId = -1
  let downPoint: { x: number; y: number } | null = null

  function canvasPoint(e: PointerEvent): { x: number; y: number } {
    const rect = canvasEl!.getBoundingClientRect()
    return { x: e.clientX - rect.left, y: e.clientY - rect.top }
  }

  function pointFromEvent(e: PointerEvent): StrokePoint {
    const { x, y } = canvasPoint(e)
    const pressure = e.pressure > 0 ? e.pressure : 0.5
    return [x, y, pressure]
  }

  function onPointerDown(e: PointerEvent): void {
    if (!canvasEl) return
    if (e.button !== 0 && e.pointerType === 'mouse') return
    // Move keyboard focus to the canvas wrapper so its onkeydown handler fires (Undo, Delete, Esc).
    containerEl?.focus({ preventScroll: true })
    canvasEl.setPointerCapture(e.pointerId)
    activePointerId = e.pointerId
    const pt = canvasPoint(e)
    downPoint = pt

    if (tool === 'pen') {
      dragMode = 'draw'
      liveStroke = {
        id: crypto.randomUUID(),
        color,
        size,
        points: [pointFromEvent(e)],
      }
      return
    }

    // Select tool
    const hit = hitTest(pt.x, pt.y)
    if (hit) {
      dragMode = 'click'
      if (e.shiftKey) {
        if (selectedIds.has(hit.id)) selectedIds.delete(hit.id)
        else selectedIds.add(hit.id)
      } else if (!selectedIds.has(hit.id)) {
        selectedIds.clear()
        selectedIds.add(hit.id)
      }
    } else {
      dragMode = 'marquee'
      if (!e.shiftKey) selectedIds.clear()
      marquee = { x0: pt.x, y0: pt.y, x1: pt.x, y1: pt.y }
    }
  }

  function onPointerMove(e: PointerEvent): void {
    if (e.pointerId !== activePointerId) return

    if (dragMode === 'draw' && liveStroke) {
      liveStroke.points = [...liveStroke.points, pointFromEvent(e)]
      return
    }

    if (dragMode === 'marquee' && marquee) {
      const pt = canvasPoint(e)
      marquee = { ...marquee, x1: pt.x, y1: pt.y }
      return
    }
  }

  function onPointerUp(e: PointerEvent): void {
    if (e.pointerId !== activePointerId) return
    const mode = dragMode
    dragMode = null
    activePointerId = -1
    const up = canvasPoint(e)
    const down = downPoint
    downPoint = null

    if (mode === 'draw' && liveStroke) {
      if (liveStroke.points.length > 0) {
        strokes = [...strokes, liveStroke]
      }
      liveStroke = null
      return
    }

    if (mode === 'marquee' && marquee && down) {
      const moved = Math.hypot(up.x - down.x, up.y - down.y) > 3
      if (moved) {
        const rx = Math.min(marquee.x0, marquee.x1)
        const ry = Math.min(marquee.y0, marquee.y1)
        const rw = Math.abs(marquee.x1 - marquee.x0)
        const rh = Math.abs(marquee.y1 - marquee.y0)
        for (const s of strokes) {
          const b = strokeBBox(s)
          const intersects = !(b.x + b.w < rx || b.x > rx + rw || b.y + b.h < ry || b.y > ry + rh)
          if (intersects) selectedIds.add(s.id)
        }
      }
      // If the marquee didn't actually move, pointerdown already cleared selection (or kept shift-anchored) — nothing more to do.
      marquee = null
      return
    }

    // 'click' mode: selection already adjusted on down; nothing to do on up.
  }

  // --- Actions ---

  function deleteSelected(): void {
    if (selectedIds.size === 0) return
    strokes = strokes.filter((s) => !selectedIds.has(s.id))
    selectedIds.clear()
  }

  function selectAll(): void {
    for (const s of strokes) selectedIds.add(s.id)
  }

  function undo(): void {
    if (strokes.length === 0) return
    const last = strokes[strokes.length - 1]
    selectedIds.delete(last.id)
    strokes = strokes.slice(0, -1)
  }

  function clear(): void {
    if (strokes.length === 0) return
    strokes = []
    selectedIds.clear()
  }

  function onKeyDown(e: KeyboardEvent): void {
    const mod = e.metaKey || e.ctrlKey

    if (mod && e.key.toLowerCase() === 'z') {
      e.preventDefault()
      undo()
      return
    }
    if (mod && e.key.toLowerCase() === 'a') {
      if (tool !== 'select') return
      e.preventDefault()
      selectAll()
      return
    }
    if ((e.key === 'Delete' || e.key === 'Backspace') && selectedIds.size > 0) {
      e.preventDefault()
      deleteSelected()
      return
    }
    if (e.key === 'Escape' && selectedIds.size > 0) {
      e.preventDefault()
      selectedIds.clear()
    }
  }

  function blobFromCanvas(canvas: HTMLCanvasElement): Promise<Blob | null> {
    return new Promise((resolve) => canvas.toBlob((b) => resolve(b), 'image/png'))
  }

  async function exportPng(): Promise<Blob | null> {
    const canvas = canvasEl
    if (!canvas) return null
    if (strokes.length === 0) {
      addToast('Drawing is empty')
      return null
    }
    // Temporarily clear selection highlights so they don't leak into export.
    const hadSelection = selectedIds.size > 0
    if (!hadSelection) return blobFromCanvas(canvas)

    const snapshot = [...selectedIds]
    selectedIds.clear()
    redraw()
    try {
      // Reuse the captured canvas reference — `canvasEl` could have been cleared
      // by an unmount between redraw() and toBlob().
      return await blobFromCanvas(canvas)
    } finally {
      for (const id of snapshot) selectedIds.add(id)
    }
  }

  async function sendToAgent(): Promise<void> {
    if (sending) return
    sending = true
    try {
      const blob = await exportPng()
      if (!blob) return

      const agent = getActiveAgentPane()
      if (!agent) {
        addToast('Open a Claude/Codex pane first')
        return
      }
      if (!agent.isRunning) {
        addToast('Agent pane is not running')
        return
      }

      try {
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
      } catch (err) {
        console.error('[drawing] clipboard.write failed:', err)
        addToast('Clipboard copy failed — check permissions')
        return
      }

      // navigator.clipboard.write resolves when the renderer accepts the write, but the
      // OS pasteboard write is performed on a separate thread. On macOS in particular,
      // a fast follow-up Ctrl+V can race the pasteboard and Claude reads stale contents.
      // 250ms is empirically enough; tradeoff vs. perceived latency.
      // TODO: replace clipboard+Ctrl-V with a direct IPC channel (e.g. agent:pasteImage) for reliability
      await new Promise((resolve) => setTimeout(resolve, 250))
      await window.api.writePty(agent.sessionId, '\x16')
      addToast(`Sent to ${agent.toolName}`)
    } finally {
      sending = false
    }
  }

  async function copyPng(): Promise<void> {
    const blob = await exportPng()
    if (!blob) return
    try {
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
      addToast('Copied PNG to clipboard')
    } catch (err) {
      console.error('[drawing] clipboard.write failed:', err)
      addToast('Clipboard copy failed')
    }
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
      <button type="button" class="danger" onclick={deleteSelected} title="Delete (Del)">
        Delete ({selectedIds.size})
      </button>
    {/if}
    <button type="button" onclick={undo} disabled={strokes.length === 0} title="Undo (⌘Z)">
      Undo
    </button>
    <button type="button" class="danger" onclick={clear} disabled={strokes.length === 0}>
      Clear
    </button>
    <button type="button" onclick={copyPng} disabled={strokes.length === 0}>Copy PNG</button>
    <button
      type="button"
      class="primary"
      onclick={sendToAgent}
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
  >
    <canvas
      bind:this={canvasEl}
      class:select-cursor={tool === 'select'}
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
    color: white;
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
    color: white;
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
  }

  canvas {
    display: block;
    touch-action: none;
    cursor: crosshair;
  }

  canvas.select-cursor {
    cursor: default;
  }
</style>
