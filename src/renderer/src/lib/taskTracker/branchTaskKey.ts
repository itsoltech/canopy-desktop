// Pure helper: extract a tracker task key (JIRA/YouTrack style, e.g. GAKKO-2754) from a branch
// name. Used as the fallback worktree→task mapping when no activeTask was persisted (branches
// created outside Canopy or before the task panel existed). Keys are uppercase-prefixed, so
// lowercase segments like "s113-foo" never match.
const TASK_KEY_IN_BRANCH_RE = /[A-Z][A-Z0-9_]*-\d+/

export function extractTaskKey(branch: string): string | null {
  const match = branch.match(TASK_KEY_IN_BRANCH_RE)
  return match ? match[0] : null
}
