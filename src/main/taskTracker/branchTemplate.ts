import type { TrackerTask, TrackerSprint } from './types'

export interface PlaceholderInfo {
  key: string
  description: string
  example: string
}

const BUILTIN_PLACEHOLDERS: PlaceholderInfo[] = [
  { key: 'branchType', description: 'Branch type prefix (feat/fix/...)', example: 'feat' },
  { key: 'sprint', description: 'Sprint number', example: '10' },
  { key: 'sprintName', description: 'Sprint name', example: 'Sprint 10' },
  { key: 'taskKey', description: 'Task key', example: 'GAKKO-21' },
  { key: 'parentKey', description: 'Parent task key (story)', example: 'GAKKO-20' },
  { key: 'taskType', description: 'Task type', example: 'subtask' },
  { key: 'taskTitle', description: 'Task title (slugified)', example: 'fix-login-bug' },
  { key: 'boardKey', description: 'Board/project key', example: 'GAKKO' },
]

const DEFAULT_TYPE_MAPPING: Record<string, string> = {
  bug: 'fix',
  story: 'feat',
  task: 'feat',
  subtask: 'feat',
  epic: 'feat',
}

export const BRANCH_TYPE_OPTIONS = ['feat', 'fix', 'refactor', 'chore', 'docs', 'test']

export function resolveBranchType(
  taskType: string,
  customMapping?: Record<string, string>,
): string {
  const mapping = { ...DEFAULT_TYPE_MAPPING, ...customMapping }
  return mapping[taskType.toLowerCase()] ?? 'feat'
}

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
    .replace(/[~^:?*[\]\\@{}#\s]/g, '-')
    .replace(/\/{2,}/g, '/')
    .replace(/^\/|\/$/g, '')
    .replace(/-+/g, '-')
    .replace(/\.lock$/g, '')
}

export function buildVariables(
  task: TrackerTask,
  sprint: TrackerSprint | null,
  customVars: Record<string, string> = {},
  branchType?: string,
): Record<string, string> {
  const vars: Record<string, string> = {
    ...(branchType !== undefined ? { branchType } : {}),
    sprint: String(sprint?.number ?? task.sprintNumber ?? ''),
    sprintName: sprint?.name ?? task.sprintName ?? '',
    taskKey: task.key,
    parentKey: task.parentKey ?? '',
    taskType: task.type,
    taskTitle: slugify(task.summary),
    boardKey: task.key.split('-')[0] ?? '',
    ...customVars,
  }
  return vars
}

export function renderBranchName(template: string, variables: Record<string, string>): string {
  let result = template.replace(
    /\{\?(\w+)\}(.*?)\{\/\1\}/g,
    (_match, varName: string, content: string) => {
      return variables[varName] ? content : ''
    },
  )

  result = result.replace(/\{(\w+)\}/g, (_match, key: string) => {
    return variables[key] ?? ''
  })

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

  // Must contain at least {taskKey}
  if (!template.includes('{taskKey}')) {
    errors.push('Template should contain {taskKey} placeholder')
  }

  return { valid: errors.length === 0, errors }
}

export function renderPreview(template: string, customVars: Record<string, string> = {}): string {
  const mockVariables: Record<string, string> = {
    branchType: 'feat',
    sprint: '10',
    sprintName: 'Sprint 10',
    taskKey: 'PROJ-42',
    parentKey: 'PROJ-40',
    taskType: 'subtask',
    taskTitle: 'fix-login-validation',
    boardKey: 'PROJ',
    ...customVars,
  }
  return renderBranchName(template, mockVariables)
}
