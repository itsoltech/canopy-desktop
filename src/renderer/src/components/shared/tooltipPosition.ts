export interface TooltipRect {
  left: number
  top: number
  width: number
  height: number
}

interface ViewportSize {
  width: number
  height: number
}

const EDGE_PADDING = 4
const TRIGGER_GAP = 4

export function tooltipPosition(
  trigger: TooltipRect,
  tooltip: Pick<TooltipRect, 'width' | 'height'>,
  viewport: ViewportSize,
): { left: number; top: number } {
  const centeredLeft = trigger.left + trigger.width / 2 - tooltip.width / 2
  const maxLeft = Math.max(EDGE_PADDING, viewport.width - tooltip.width - EDGE_PADDING)
  const left = Math.max(EDGE_PADDING, Math.min(centeredLeft, maxLeft))

  const below = trigger.top + trigger.height + TRIGGER_GAP
  const above = trigger.top - tooltip.height - TRIGGER_GAP
  const maxTop = Math.max(EDGE_PADDING, viewport.height - tooltip.height - EDGE_PADDING)
  const preferredTop = below + tooltip.height <= viewport.height - EDGE_PADDING ? below : above
  const top = Math.max(EDGE_PADDING, Math.min(preferredTop, maxTop))

  return { left, top }
}
