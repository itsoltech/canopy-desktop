import { workspaceState, getProjectForWorktree } from './workspace.svelte'

export type NoteScope = 'project' | 'worktree'

/** Note content keyed by repoRoot (project scope) or worktreePath (worktree scope). */
export const notesState: Record<string, string> = $state({})

/** Per-pane UI toggle — remembers last-selected scope for each Notes pane instance. */
export const notesUiScope: Record<string, NoteScope> = $state({})

export function getNoteKey(scope: NoteScope): string | null {
  const wt = workspaceState.selectedWorktreePath
  if (!wt) return null
  if (scope === 'project') {
    const p = getProjectForWorktree(wt)
    return p?.repoRoot ?? p?.workspace.path ?? null
  }
  return wt
}

export function getNoteLabel(scope: NoteScope): string {
  const wt = workspaceState.selectedWorktreePath
  if (!wt) return ''
  if (scope === 'project') {
    const p = getProjectForWorktree(wt)
    return p?.workspace.name ?? ''
  }
  const p = getProjectForWorktree(wt)
  const branch = p?.worktrees.find((w) => w.path === wt)?.branch
  return branch ?? wt.split('/').pop() ?? ''
}
