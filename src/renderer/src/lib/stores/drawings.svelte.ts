import { workspaceState, getProjectForWorktree } from './workspace.svelte'

export type DrawTool = 'pen' | 'select'

/** A single input point captured from pointer events. */
export type StrokePoint = [x: number, y: number, pressure: number]

export interface Stroke {
  id: string
  color: string
  size: number
  points: StrokePoint[]
}

/** Snapshot persisted per project. Stored as plain arrays — trivially serializable. */
export const drawingsState: Record<string, Stroke[]> = $state({})

export function getDrawingKey(): string | null {
  const wt = workspaceState.selectedWorktreePath
  if (!wt) return null
  const p = getProjectForWorktree(wt)
  return p?.repoRoot ?? p?.workspace.path ?? wt
}
