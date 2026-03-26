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

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="split-divider"
  class:horizontal={direction === 'horizontal'}
  class:vertical={direction === 'vertical'}
  class:dragging
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerUp}
></div>

<style>
  .split-divider {
    width: 100%;
    height: 100%;
    background: rgba(255, 255, 255, 0.06);
    transition: background 0.15s;
    z-index: 5;
  }

  .split-divider:hover,
  .split-divider.dragging {
    background: rgba(116, 192, 252, 0.3);
  }

  .split-divider.vertical {
    cursor: col-resize;
  }

  .split-divider.horizontal {
    cursor: row-resize;
  }
</style>
