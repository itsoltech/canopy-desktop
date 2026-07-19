// Pure helpers: extract tracker task keys (JIRA/YouTrack style, e.g. GAKKO-2754) from a branch
// name. Used as the fallback worktree→task mapping when no activeTask was persisted (branches
// created outside Canopy or before the task panel existed). Keys are uppercase-prefixed, so
// lowercase segments like "s113-foo" never match.
const TASK_KEY_IN_BRANCH_RE = /[A-Z][A-Z0-9_]*-\d+/g

/** All distinct task keys in order of appearance (e.g. parent/subtask branches carry several). */
export function extractTaskKeys(branch: string): string[] {
  const keys: string[] = []
  for (const m of branch.matchAll(TASK_KEY_IN_BRANCH_RE)) {
    if (!keys.includes(m[0])) keys.push(m[0])
  }
  return keys
}

export function extractTaskKey(branch: string): string | null {
  return extractTaskKeys(branch)[0] ?? null
}
