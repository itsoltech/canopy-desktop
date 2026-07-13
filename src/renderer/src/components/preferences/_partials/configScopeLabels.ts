// Pure helpers + shared constants for the task-tracker preferences UI.
// Kept pure (no IO, no Svelte state) so they can be unit-tested under Vitest.

// Built-in template presets — used to prefill a new template and as the panel fallback when a
// scope hasn't set a value. Built-in is NOT a config tier the user manages; it is only the seed.
// Keep these in sync with src/main/taskTracker/configDefaults.ts (DEFAULT_BRANCH_TEMPLATE /
// DEFAULT_PR_TEMPLATE) — that file is the canonical source; we can't import main-process modules
// into the renderer.
export const RENDERER_DEFAULT_BRANCH_TEMPLATE = '{branchType}/{taskKey}-{taskTitle}'
export const RENDERER_DEFAULT_PR_TITLE = '[{taskKey}] {taskTitle}'
export const RENDERER_DEFAULT_PR_BODY = '## {taskKey}: {taskTitle}\n\n{taskUrl}'

// Sample task values used to render illustrative examples next to templates. Branch values are
// slugified (branch names are), PR values keep natural casing.
export const BRANCH_EXAMPLE_VALUES: Record<string, string> = {
  branchType: 'feature',
  sprint: '14',
  sprintName: 'sprint-14',
  taskKey: 'ISSUE-123',
  parentKey: 'ISSUE-100',
  taskType: 'task',
  taskTitle: 'fix-login-bug',
  boardKey: 'ISSUE',
}

export const PR_EXAMPLE_VALUES: Record<string, string> = {
  taskKey: 'ISSUE-123',
  parentKey: 'ISSUE-100',
  taskType: 'task',
  taskTitle: 'Fix login bug',
  taskUrl: 'https://tracker.example.com/browse/ISSUE-123',
  taskDescription: 'Login form does not validate email format',
  boardKey: 'ISSUE',
}

/** Substitute {field} placeholders with sample values; unknown fields are left as-is. */
export function renderTemplateExample(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_m, k: string) => values[k] ?? `{${k}}`)
}

/**
 * Repo-origin trackers that have no stored credentials. Drives the "needs credentials" alert
 * (sidebar) — only trackers defined in the project's .canopy/config.json count, per the agreed
 * business rule; personal-only trackers are surfaced inside Connections but not here.
 */
export function trackersNeedingCredentials<T extends { id: string }>(
  repoTrackers: readonly T[],
  hasCreds: (id: string) => boolean,
): T[] {
  return repoTrackers.filter((t) => !hasCreds(t.id))
}
