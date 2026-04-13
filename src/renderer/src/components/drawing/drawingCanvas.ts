import { getStroke } from 'perfect-freehand'
import type { Stroke, StrokePoint } from '../../lib/stores/drawings.svelte'

export function strokePath(stroke: Stroke): Path2D {
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

export function strokeBBox(stroke: Stroke): { x: number; y: number; w: number; h: number } {
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

export function hitTest(
  px: number,
  py: number,
  ctx: CanvasRenderingContext2D,
  strokes: Stroke[],
  dpr: number,
): Stroke | null {
  for (let i = strokes.length - 1; i >= 0; i--) {
    if (ctx.isPointInPath(strokePath(strokes[i]), px * dpr, py * dpr)) {
      return strokes[i]
    }
  }
  return null
}

export interface RedrawParams {
  canvas: HTMLCanvasElement
  container: HTMLElement
  dpr: number
  width: number
  height: number
  strokes: Stroke[]
  liveStroke: Stroke | null
  selectedIds: ReadonlySet<string>
  marquee: { x0: number; y0: number; x1: number; y1: number } | null
}

function getThemeColor(container: HTMLElement, prop: string, fallback: string): string {
  return getComputedStyle(container).getPropertyValue(prop).trim() || fallback
}

export function redraw(params: RedrawParams): void {
  const ctx = params.canvas.getContext('2d')
  if (!ctx) return

  const bg = getThemeColor(params.container, '--c-bg', '#1e1e1e')
  const accent = getThemeColor(params.container, '--c-accent', '#60a5fa')
  const accentBg = getThemeColor(params.container, '--c-accent-bg', 'rgba(96, 165, 250, 0.12)')

  ctx.setTransform(params.dpr, 0, 0, params.dpr, 0, 0)
  ctx.fillStyle = bg
  ctx.fillRect(0, 0, params.width, params.height)

  for (const s of params.strokes) {
    const path = strokePath(s)
    ctx.fillStyle = s.color
    ctx.fill(path)
    if (params.selectedIds.has(s.id)) {
      ctx.save()
      ctx.strokeStyle = accent
      ctx.lineWidth = 2
      ctx.stroke(path)
      ctx.restore()
    }
  }
  if (params.liveStroke) {
    ctx.fillStyle = params.liveStroke.color
    ctx.fill(strokePath(params.liveStroke))
  }
  if (params.marquee) {
    ctx.save()
    const x = Math.min(params.marquee.x0, params.marquee.x1)
    const y = Math.min(params.marquee.y0, params.marquee.y1)
    const w = Math.abs(params.marquee.x1 - params.marquee.x0)
    const h = Math.abs(params.marquee.y1 - params.marquee.y0)
    ctx.fillStyle = accentBg
    ctx.strokeStyle = accent
    ctx.lineWidth = 1
    ctx.fillRect(x, y, w, h)
    ctx.strokeRect(x + 0.5, y + 0.5, w, h)
    ctx.restore()
  }
}

export function canvasPoint(e: PointerEvent, canvas: HTMLCanvasElement): { x: number; y: number } {
  const rect = canvas.getBoundingClientRect()
  return { x: e.clientX - rect.left, y: e.clientY - rect.top }
}

export function pointFromEvent(e: PointerEvent, canvas: HTMLCanvasElement): StrokePoint {
  const { x, y } = canvasPoint(e, canvas)
  const pressure = e.pressure > 0 ? e.pressure : 0.5
  return [x, y, pressure]
}

export function blobFromCanvas(canvas: HTMLCanvasElement): Promise<Blob | null> {
  return new Promise((resolve) => canvas.toBlob((b) => resolve(b), 'image/png'))
}
