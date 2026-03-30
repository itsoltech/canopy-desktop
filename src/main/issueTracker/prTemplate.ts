import type { PRTargetRule, TrackerIssue } from './types'

export interface PRRenderResult {
  title: string
  body: string
  targetBranch: string
}

export function renderPRTitle(template: string, issue: TrackerIssue): string {
  return replacePlaceholders(template, issue)
}

export function renderPRBody(template: string, issue: TrackerIssue): string {
  return replacePlaceholders(template, issue)
}

function replacePlaceholders(template: string, issue: TrackerIssue): string {
  return template
    .replace(/\{issueKey\}/g, issue.key)
    .replace(/\{issueTitle\}/g, issue.summary)
    .replace(/\{issueType\}/g, issue.type)
    .replace(/\{parentKey\}/g, issue.parentKey ?? '')
    .replace(/\{boardKey\}/g, issue.key.split('-')[0] ?? '')
    .replace(/\{issueUrl\}/g, issue.url ?? '')
    .replace(/\{issueDescription\}/g, issue.description ?? '')
}

export function resolveTargetBranch(
  issue: TrackerIssue,
  defaultBranch: string,
  targetRules: PRTargetRule[],
  existingBranches?: string[],
): string {
  const matchingRule = targetRules.find(
    (rule) => rule.issueType.toLowerCase() === issue.type.toLowerCase(),
  )

  if (!matchingRule) return defaultBranch

  const target = matchingRule.targetPattern
    .replace(/\{parentKey\}/g, issue.parentKey ?? '')
    .replace(/\{issueKey\}/g, issue.key)
    .replace(/\{boardKey\}/g, issue.key.split('-')[0] ?? '')

  // If target references a branch pattern, try to find it in existing branches
  if (existingBranches && target.includes('/')) {
    const found = existingBranches.find((b) => b.includes(target) || target.includes(b))
    if (found) return found
  }

  return target || defaultBranch
}

export function renderPR(
  issue: TrackerIssue,
  titleTemplate: string,
  bodyTemplate: string,
  defaultBranch: string,
  targetRules: PRTargetRule[],
  existingBranches?: string[],
): PRRenderResult {
  return {
    title: renderPRTitle(titleTemplate, issue),
    body: renderPRBody(bodyTemplate, issue),
    targetBranch: resolveTargetBranch(issue, defaultBranch, targetRules, existingBranches),
  }
}

export function renderPRPreview(
  titleTemplate: string,
  bodyTemplate: string,
): { title: string; body: string } {
  const mockIssue: TrackerIssue = {
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
    title: renderPRTitle(titleTemplate, mockIssue),
    body: renderPRBody(bodyTemplate, mockIssue),
  }
}
