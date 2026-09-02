<script lang="ts">
  import { onDestroy } from 'svelte'

  let {
    direction,
    onDragDelta,
  }: {
    direction: 'horizontal' | 'vertical'
    onDragDelta: (deltaPx: number) => void
  } = $props()

  let dragging = $state(false)
  let startPos = 0
  let pendingDelta = 0
  let rafId: number | null = null

  function flushDelta(): void {
    rafId = null
    const delta = pendingDelta
    pendingDelta = 0
    if (delta !== 0) onDragDelta(delta)
  }

  function handlePointerDown(e: PointerEvent): void {
    e.preventDefault()
    const target = e.currentTarget as HTMLElement
    target.setPointerCapture(e.pointerId)
    dragging = true
    startPos = direction === 'vertical' ? e.clientX : e.clientY
  }

  function handlePointerMove(e: PointerEvent): void {
    if (!dragging) return
    const currentPos = direction === 'vertical' ? e.clientX : e.clientY
    const delta = currentPos - startPos
    if (delta === 0) return
    startPos = currentPos
    // Coalesce to one update per frame. Each onDragDelta reaches an IPC round
    // trip plus a layout-persist schedule, and a drag emits pointermove far
    // faster than we can paint, so sending every event lags the divider behind
    // the cursor. Deltas are relative, so accumulating them loses nothing.
    pendingDelta += delta
    rafId ??= requestAnimationFrame(flushDelta)
  }

  function handlePointerUp(): void {
    dragging = false
    // Commit the tail of the gesture that hasn't been flushed yet.
    if (rafId !== null) {
      cancelAnimationFrame(rafId)
      flushDelta()
    }
  }

  onDestroy(() => {
    if (rafId !== null) cancelAnimationFrame(rafId)
  })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="w-full h-full bg-border-subtle transition-colors duration-base z-pane-divider hover:bg-accent-muted"
  class:bg-accent-muted={dragging}
  class:cursor-col-resize={direction === 'vertical'}
  class:cursor-row-resize={direction === 'horizontal'}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerUp}
></div>
