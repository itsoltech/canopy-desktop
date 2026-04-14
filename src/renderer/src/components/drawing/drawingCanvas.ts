import { getStroke } from 'perfect-freehand'
import { match, P } from 'ts-pattern'
import type { Stroke, ShapeStroke, StrokePoint } from '../../lib/stores/drawings.svelte'

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

export function drawShape(ctx: CanvasRenderingContext2D, stroke: ShapeStroke): void {
  const { x, y, w, h } = stroke.rect
  ctx.strokeStyle = stroke.color
  ctx.lineWidth = stroke.size
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'

  match(stroke.type)
    .with('rectangle', () => {
      ctx.strokeRect(x, y, w, h)
    })
    .with('ellipse', () => {
      const cx = x + w / 2
      const cy = y + h / 2
      const rx = Math.abs(w / 2)
      const ry = Math.abs(h / 2)
      ctx.beginPath()
      ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2)
      ctx.stroke()
    })
    .with('line', () => {
      ctx.beginPath()
      ctx.moveTo(x, y)
      ctx.lineTo(x + w, y + h)
      ctx.stroke()
    })
    .with('arrow', () => {
      const ex = x + w
      const ey = y + h
      const angle = Math.atan2(h, w)
      const headLen = Math.max(stroke.size * 4, 14)
      const stopX = ex - headLen * 0.6 * Math.cos(angle)
      const stopY = ey - headLen * 0.6 * Math.sin(angle)
      ctx.beginPath()
      ctx.moveTo(x, y)
      ctx.lineTo(stopX, stopY)
      ctx.stroke()
      ctx.beginPath()
      ctx.moveTo(ex, ey)
      ctx.lineTo(
        ex - headLen * Math.cos(angle - Math.PI / 6),
        ey - headLen * Math.sin(angle - Math.PI / 6),
      )
      ctx.lineTo(
        ex - headLen * Math.cos(angle + Math.PI / 6),
        ey - headLen * Math.sin(angle + Math.PI / 6),
      )
      ctx.closePath()
      ctx.fillStyle = stroke.color
      ctx.fill()
      ctx.stroke()
    })
    .exhaustive()
}

export function hitTestShape(px: number, py: number, stroke: ShapeStroke): boolean {
  const { x, y, w, h } = stroke.rect
  const tolerance = Math.max(stroke.size, 6)

  return match(stroke.type)
    .with('rectangle', () => {
      const minX = Math.min(x, x + w)
      const minY = Math.min(y, y + h)
      const maxX = Math.max(x, x + w)
      const maxY = Math.max(y, y + h)
      const nearLeft =
        Math.abs(px - minX) < tolerance && py >= minY - tolerance && py <= maxY + tolerance
      const nearRight =
        Math.abs(px - maxX) < tolerance && py >= minY - tolerance && py <= maxY + tolerance
      const nearTop =
        Math.abs(py - minY) < tolerance && px >= minX - tolerance && px <= maxX + tolerance
      const nearBottom =
        Math.abs(py - maxY) < tolerance && px >= minX - tolerance && px <= maxX + tolerance
      return nearLeft || nearRight || nearTop || nearBottom
    })
    .with('ellipse', () => {
      const cx = x + w / 2
      const cy = y + h / 2
      const rx = Math.abs(w / 2)
      const ry = Math.abs(h / 2)
      if (rx < 1 || ry < 1) return false
      const norm = ((px - cx) / rx) ** 2 + ((py - cy) / ry) ** 2
      return Math.abs(Math.sqrt(norm) - 1) * Math.min(rx, ry) < tolerance
    })
    .with(P.union('line', 'arrow'), () => {
      const ex = x + w
      const ey = y + h
      const dx = ex - x
      const dy = ey - y
      const lenSq = dx * dx + dy * dy
      if (lenSq < 1) return Math.hypot(px - x, py - y) < tolerance
      const t = Math.max(0, Math.min(1, ((px - x) * dx + (py - y) * dy) / lenSq))
      const closestX = x + t * dx
      const closestY = y + t * dy
      return Math.hypot(px - closestX, py - closestY) < tolerance
    })
    .exhaustive()
}

export function strokeBBox(stroke: Stroke): { x: number; y: number; w: number; h: number } {
  if (stroke.type !== 'freehand') {
    const { x, y, w, h } = stroke.rect
    const minX = Math.min(x, x + w)
    const minY = Math.min(y, y + h)
    const maxX = Math.max(x, x + w)
    const maxY = Math.max(y, y + h)
    const pad = stroke.size
    return { x: minX - pad, y: minY - pad, w: maxX - minX + 2 * pad, h: maxY - minY + 2 * pad }
  }
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
    const s = strokes[i]
    if (s.type !== 'freehand') {
      if (hitTestShape(px, py, s)) return s
    } else {
      ctx.save()
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      const hit = ctx.isPointInPath(strokePath(s), px * dpr, py * dpr)
      ctx.restore()
      if (hit) return s
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
  panX: number
  panY: number
  strokes: Stroke[]
  liveStroke: Stroke | null
  selectedIds: ReadonlySet<string>
  marquee: { x0: number; y0: number; x1: number; y1: number } | null
}

function getThemeColor(container: HTMLElement, prop: string, fallback: string): string {
  const value = getComputedStyle(container).getPropertyValue(prop).trim()
  if (value) return value
  // Fall back to document root where theme vars are defined, in case container isn't styled yet
  return getComputedStyle(document.documentElement).getPropertyValue(prop).trim() || fallback
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

  ctx.translate(params.panX, params.panY)

  for (const s of params.strokes) {
    if (s.type !== 'freehand') {
      ctx.save()
      drawShape(ctx, s)
      if (params.selectedIds.has(s.id)) {
        ctx.strokeStyle = accent
        ctx.lineWidth = 2
        const bb = strokeBBox(s)
        ctx.strokeRect(bb.x, bb.y, bb.w, bb.h)
      }
      ctx.restore()
    } else {
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
  }
  if (params.liveStroke) {
    if (params.liveStroke.type !== 'freehand') {
      ctx.save()
      drawShape(ctx, params.liveStroke)
      ctx.restore()
    } else {
      ctx.fillStyle = params.liveStroke.color
      ctx.fill(strokePath(params.liveStroke))
    }
  }
  if (params.marquee) {
    ctx.save()
    ctx.setTransform(params.dpr, 0, 0, params.dpr, 0, 0)
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

export function canvasPoint(
  e: PointerEvent,
  canvas: HTMLCanvasElement,
  panX = 0,
  panY = 0,
): { x: number; y: number } {
  const rect = canvas.getBoundingClientRect()
  return { x: e.clientX - rect.left - panX, y: e.clientY - rect.top - panY }
}

export function screenPoint(e: PointerEvent, canvas: HTMLCanvasElement): { x: number; y: number } {
  const rect = canvas.getBoundingClientRect()
  return { x: e.clientX - rect.left, y: e.clientY - rect.top }
}

export function pointFromEvent(
  e: PointerEvent,
  canvas: HTMLCanvasElement,
  panX = 0,
  panY = 0,
): StrokePoint {
  const { x, y } = canvasPoint(e, canvas, panX, panY)
  const pressure = e.pressure > 0 ? e.pressure : 0.5
  return [x, y, pressure]
}

export function blobFromCanvas(canvas: HTMLCanvasElement): Promise<Blob | null> {
  return new Promise((resolve) => canvas.toBlob((b) => resolve(b), 'image/png'))
}
