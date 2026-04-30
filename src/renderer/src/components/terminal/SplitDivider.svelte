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
      onDragDelta(delta)
      startPos = currentPos
    }
  }

  function handlePointerUp(): void {
    dragging = false
  }
</script>

<div
  role="separator"
  aria-orientation={direction === 'vertical' ? 'vertical' : 'horizontal'}
  aria-label="Resize panes"
  class="w-full h-full bg-border-subtle transition-colors duration-base z-pane-divider hover:bg-accent-muted"
  class:bg-accent-muted={dragging}
  class:cursor-col-resize={direction === 'vertical'}
  class:cursor-row-resize={direction === 'horizontal'}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerUp}
></div>
