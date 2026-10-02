import type { PRTargetRule, TrackerTask } from './types'

export interface PRRenderResult {
  title: string
  body: string
  targetBranch: string
}

export function renderPRTitle(template: string, task: TrackerTask): string {
  return replacePlaceholders(template, task)
}

export function renderPRBody(template: string, task: TrackerTask): string {
  return replacePlaceholders(template, task)
}

function replacePlaceholders(template: string, task: TrackerTask): string {
  // Tasks can arrive partially hydrated over IPC — never inject "undefined" into a PR.
  return template
    .replace(/\{taskKey\}/g, task.key ?? '')
    .replace(/\{taskTitle\}/g, task.summary ?? '')
    .replace(/\{taskType\}/g, task.type ?? '')
    .replace(/\{parentKey\}/g, task.parentKey ?? '')
    .replace(/\{boardKey\}/g, task.key?.split('-')[0] ?? '')
    .replace(/\{taskUrl\}/g, task.url ?? '')
    .replace(/\{taskDescription\}/g, task.description ?? '')
}

export function resolveTargetBranch(
  task: TrackerTask,
  defaultBranch: string,
  targetRules: PRTargetRule[],
  existingBranches?: string[],
): string {
  const matchingRule = targetRules.find(
    (rule) => rule.taskType.toLowerCase() === task.type.toLowerCase(),
  )

  if (!matchingRule) return defaultBranch
  // A task without a parent would leave a dangling `feature/` — not a branch to target.
  if (matchingRule.targetPattern.includes('{parentKey}') && !task.parentKey) return defaultBranch

  const target = matchingRule.targetPattern
    .replace(/\{parentKey\}/g, task.parentKey ?? '')
    .replace(/\{taskKey\}/g, task.key)
    .replace(/\{boardKey\}/g, task.key.split('-')[0] ?? '')

  // If target references a branch pattern, try to find it in existing branches
  if (existingBranches && target.includes('/')) {
    const found = findExistingBranch(target, existingBranches)
    if (found) return found
  }

  return target || defaultBranch
}

/**
 * An exact branch, else one containing the target as whole path segments (`feature/ABC-10-login`,
 * `alice/feature/ABC-10`), so `feature/ABC-10` resolves to neither `feature/ABC-1` nor
 * `feature/ABC-100`.
 */
function findExistingBranch(target: string, existingBranches: string[]): string | undefined {
  const escaped = target.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const segmentAligned = new RegExp(`(?:^|/)${escaped}(?:$|[-_./])`)
  return (
    existingBranches.find((b) => b === target) ??
    existingBranches.find((b) => segmentAligned.test(b))
  )
}

export function renderPR(
  task: TrackerTask,
  titleTemplate: string,
  bodyTemplate: string,
  defaultBranch: string,
  targetRules: PRTargetRule[],
  existingBranches?: string[],
): PRRenderResult {
  return {
    title: renderPRTitle(titleTemplate, task),
    body: renderPRBody(bodyTemplate, task),
    targetBranch: resolveTargetBranch(task, defaultBranch, targetRules, existingBranches),
  }
}

export function renderPRPreview(
  titleTemplate: string,
  bodyTemplate: string,
): { title: string; body: string } {
  const mockTask: TrackerTask = {
    key: 'PROJ-42',
    summary: 'Fix login validation',
    description: 'Login form does not validate email format',
    status: 'To Do',
    priority: 'High',
    type: 'task',
    parentKey: 'PROJ-40',
    url: 'https://example.atlassian.net/browse/PROJ-42',
  }
  return {
    title: renderPRTitle(titleTemplate, mockTask),
    body: renderPRBody(bodyTemplate, mockTask),
  }
}
