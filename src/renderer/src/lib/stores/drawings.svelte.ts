import { workspaceState, getProjectForWorktree } from './workspace.svelte'

export type ShapeType = 'rectangle' | 'ellipse' | 'line' | 'arrow'
export type DrawTool = 'pen' | 'select' | 'rectangle' | 'ellipse' | 'line' | 'arrow'

/** A single input point captured from pointer events. */
export type StrokePoint = [x: number, y: number, pressure: number]

export interface ShapeRect {
  x: number
  y: number
  w: number
  h: number
}

export type FreehandStroke = {
  id: string
  color: string
  size: number
  type: 'freehand'
  points: StrokePoint[]
}

export type ShapeStroke = {
  id: string
  color: string
  size: number
  type: ShapeType
  rect: ShapeRect
}

export type Stroke = FreehandStroke | ShapeStroke

/** Snapshot persisted per project. Stored as plain arrays — trivially serializable. */
export const drawingsState: Record<string, Stroke[]> = $state({})

export function getDrawingKey(): string | null {
  const wt = workspaceState.selectedWorktreePath
  if (!wt) return null
  const p = getProjectForWorktree(wt)
  return p?.repoRoot ?? p?.workspace.path ?? wt
}
