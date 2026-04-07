import { screen } from 'electron'

export interface WindowBounds {
  x: number
  y: number
  width: number
  height: number
}

export type WindowState = 'normal' | 'maximized' | 'fullscreen'

export interface WindowConfig {
  paths: string[]
  activeWorktreePath?: string
  bounds?: WindowBounds
  windowState?: WindowState
}

const MIN_WIDTH = 600
const MIN_HEIGHT = 400
const MIN_VISIBLE_PX = 100
const CASCADE_OFFSET = 30

function computeOverlap(
  a: { x: number; y: number; width: number; height: number },
  b: { x: number; y: number; width: number; height: number },
): { overlapX: number; overlapY: number } {
  const overlapX = Math.max(0, Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x))
  const overlapY = Math.max(0, Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y))
  return { overlapX, overlapY }
}

function centerOnPrimary(width: number, height: number): WindowBounds {
  const primary = screen.getPrimaryDisplay().workArea
  const w = Math.min(width, primary.width)
  const h = Math.min(height, primary.height)
  return {
    x: primary.x + Math.round((primary.width - w) / 2),
    y: primary.y + Math.round((primary.height - h) / 2),
    width: w,
    height: h,
  }
}

/** Validate saved bounds against current display layout. Returns usable bounds. */
export function validateBounds(bounds: WindowBounds): WindowBounds {
  const width = Math.max(bounds.width, MIN_WIDTH)
  const height = Math.max(bounds.height, MIN_HEIGHT)
  const candidate = { x: bounds.x, y: bounds.y, width, height }

  const displays = screen.getAllDisplays()
  for (const display of displays) {
    const { overlapX, overlapY } = computeOverlap(candidate, display.workArea)
    if (overlapX >= MIN_VISIBLE_PX && overlapY >= MIN_VISIBLE_PX) {
      return candidate
    }
  }

  return centerOnPrimary(width, height)
}

/** Compute cascaded bounds offset from the last focused window. */
export function cascadeBounds(lastBounds: WindowBounds | null): WindowBounds | undefined {
  if (!lastBounds) return undefined

  const candidate: WindowBounds = {
    x: lastBounds.x + CASCADE_OFFSET,
    y: lastBounds.y + CASCADE_OFFSET,
    width: lastBounds.width,
    height: lastBounds.height,
  }

  const displays = screen.getAllDisplays()
  for (const display of displays) {
    const { overlapX, overlapY } = computeOverlap(candidate, display.workArea)
    if (overlapX >= MIN_VISIBLE_PX && overlapY >= MIN_VISIBLE_PX) {
      return candidate
    }
  }

  // Cascaded position is off-screen — wrap to top-left of primary workArea
  const primary = screen.getPrimaryDisplay().workArea
  return {
    x: primary.x,
    y: primary.y,
    width: Math.min(candidate.width, primary.width),
    height: Math.min(candidate.height, primary.height),
  }
}
