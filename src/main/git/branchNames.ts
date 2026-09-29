/**
 * Branch names from `git branch` output. The current branch is prefixed with `*` and, since
 * git 2.23, a branch checked out in another worktree with `+` — leaving that marker in place
 * made every linked worktree's own branch look unmerged.
 */
export function parseBranchNames(raw: string): string[] {
  return raw
    .split('\n')
    .map((line) => line.replace(/^[*+]?\s+/, '').trim())
    .filter(Boolean)
}
