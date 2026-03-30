import type { TrackerIssue, TrackerSprint } from './types'

export interface PlaceholderInfo {
  key: string
  description: string
  example: string
}

const BUILTIN_PLACEHOLDERS: PlaceholderInfo[] = [
  { key: 'sprint', description: 'Sprint number', example: '10' },
  { key: 'sprintName', description: 'Sprint name', example: 'Sprint 10' },
  { key: 'issueKey', description: 'Issue key', example: 'GAKKO-21' },
  { key: 'parentKey', description: 'Parent issue key (story)', example: 'GAKKO-20' },
  { key: 'issueType', description: 'Issue type', example: 'subtask' },
  { key: 'issueTitle', description: 'Issue title (slugified)', example: 'fix-login-bug' },
  { key: 'boardKey', description: 'Board/project key', example: 'GAKKO' },
]

export function getAvailablePlaceholders(
  customVars: Record<string, string> = {},
): PlaceholderInfo[] {
  const custom = Object.entries(customVars).map(
    ([key, value]): PlaceholderInfo => ({
      key,
      description: `Custom: ${key}`,
      example: value,
    }),
  )
  return [...BUILTIN_PLACEHOLDERS, ...custom]
}

function slugify(text: string, maxLength = 50): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, maxLength)
}

function sanitizeBranchName(name: string): string {
  return name
    .replace(/\.\./g, '.')
    .replace(/[~^:?*[\]\\@{}\s]/g, '-')
    .replace(/\/\//g, '/')
    .replace(/\/{2,}/g, '/')
    .replace(/^\/|\/$/g, '')
    .replace(/-+/g, '-')
    .replace(/\.lock$/g, '')
}

export function buildVariables(
  issue: TrackerIssue,
  sprint: TrackerSprint | null,
  customVars: Record<string, string> = {},
): Record<string, string> {
  const vars: Record<string, string> = {
    sprint: String(sprint?.number ?? issue.sprintNumber ?? ''),
    sprintName: sprint?.name ?? issue.sprintName ?? '',
    issueKey: issue.key,
    parentKey: issue.parentKey ?? '',
    issueType: issue.type,
    issueTitle: slugify(issue.summary),
    boardKey: issue.key.split('-')[0] ?? '',
    ...customVars,
  }
  return vars
}

export function renderBranchName(template: string, variables: Record<string, string>): string {
  // Handle conditional segments: {?varName}content{/varName}
  let result = template.replace(
    /\{\?(\w+)\}(.*?)\{\/\1\}/g,
    (_match, varName: string, content: string) => {
      return variables[varName] ? content : ''
    },
  )

  // Replace placeholders
  result = result.replace(/\{(\w+)\}/g, (_match, key: string) => {
    return variables[key] ?? ''
  })

  // Clean up empty segments
  result = result.replace(/\/+/g, '/').replace(/^\/|\/$/g, '')

  return sanitizeBranchName(result)
}

export function validateTemplate(template: string): { valid: boolean; errors: string[] } {
  const errors: string[] = []

  if (!template.trim()) {
    errors.push('Template cannot be empty')
    return { valid: false, errors }
  }

  // Check for unclosed placeholders
  const openBraces = (template.match(/\{/g) || []).length
  const closeBraces = (template.match(/\}/g) || []).length
  if (openBraces !== closeBraces) {
    errors.push('Mismatched braces in template')
  }

  // Check for unknown placeholders
  const knownKeys = new Set(BUILTIN_PLACEHOLDERS.map((p) => p.key))
  const usedKeys = template.match(/\{(\w+)\}/g) || []
  for (const match of usedKeys) {
    const key = match.slice(1, -1)
    if (key.startsWith('?') || key.startsWith('/')) continue
    if (!knownKeys.has(key)) {
      // Not an error — could be a custom variable
    }
  }

  // Must contain at least {issueKey}
  if (!template.includes('{issueKey}')) {
    errors.push('Template should contain {issueKey} placeholder')
  }

  return { valid: errors.length === 0, errors }
}

export function renderPreview(template: string, customVars: Record<string, string> = {}): string {
  const mockVariables: Record<string, string> = {
    sprint: '10',
    sprintName: 'Sprint 10',
    issueKey: 'PROJ-42',
    parentKey: 'PROJ-40',
    issueType: 'subtask',
    issueTitle: 'fix-login-validation',
    boardKey: 'PROJ',
    ...customVars,
  }
  return renderBranchName(template, mockVariables)
}
