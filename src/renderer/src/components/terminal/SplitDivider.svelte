<script lang="ts">
  let {
    direction,
    onDragDelta,
  }: {
    direction: 'horizontal' | 'vertical'
    onDragDelta: (deltaPx: number) => void
  } = $props()

  let dragging = $state(false)
  let startPos = 0

  // Each onDragDelta lands a ratio update that round-trips to the main process
  // and rebuilds the tab snapshot. Raw pointermove fires per input sample —
  // 60Hz on a plain display, far more on a high-polling mouse — so the deltas
  // are accumulated and flushed once per frame. They are relative increments,
  // so a summed flush moves the divider exactly as far as the individual ones.
  let pendingDelta = 0
  let rafId: number | null = null

  function flushDelta(): void {
    rafId = null
    if (pendingDelta === 0) return
    onDragDelta(pendingDelta)
    pendingDelta = 0
  }

  function cancelPendingFlush(): void {
    if (rafId !== null) {
      cancelAnimationFrame(rafId)
      rafId = null
    }
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
    if (delta !== 0) {
      pendingDelta += delta
      startPos = currentPos
      rafId ??= requestAnimationFrame(flushDelta)
    }
  }

  function handlePointerUp(): void {
    dragging = false
    // Land the tail of the drag; without this the last few pixels before
    // release would be dropped along with the pending frame.
    cancelPendingFlush()
    flushDelta()
  }

  // Drop a scheduled frame if the divider unmounts mid-drag (pane closed).
  $effect(() => {
    return () => cancelPendingFlush()
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
