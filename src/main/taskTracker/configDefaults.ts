import type { BranchTemplateConfig, PRTemplateConfig, RepoConfig } from './types'

export const DEFAULT_BRANCH_TEMPLATE: BranchTemplateConfig & {
  typeMapping?: Record<string, string>
} = {
  template: '{branchType}/{taskKey}-{taskTitle}',
  customVars: {},
}

export const DEFAULT_PR_TEMPLATE: PRTemplateConfig = {
  titleTemplate: '[{taskKey}] {taskTitle}',
  bodyTemplate: '## {taskKey}: {taskTitle}\n\n{taskUrl}',
  defaultTargetBranch: '',
  targetRules: [],
}

export function defaultConfig(): RepoConfig {
  return {
    version: 1 as const,
    trackers: [],
    boardOverrides: {},
    filters: {
      assignedToMe: true,
      statuses: [],
    },
  }
}

export function getBranchTemplate(
  config: RepoConfig,
  boardId?: string,
): BranchTemplateConfig & { typeMapping?: Record<string, string> } {
  const base = config.branchTemplate ?? DEFAULT_BRANCH_TEMPLATE
  if (boardId) {
    const override = config.boardOverrides[boardId]?.branchTemplate
    if (override) {
      return {
        template: override.template ?? base.template,
        customVars: { ...base.customVars, ...override.customVars },
        typeMapping: override.typeMapping ?? base.typeMapping,
      }
    }
  }
  return base
}

export function getPRTemplate(config: RepoConfig, boardId?: string): PRTemplateConfig {
  const base = config.prTemplate ?? DEFAULT_PR_TEMPLATE
  if (boardId) {
    const override = config.boardOverrides[boardId]?.prTemplate
    if (override) {
      return {
        titleTemplate: override.titleTemplate ?? base.titleTemplate,
        bodyTemplate: override.bodyTemplate ?? base.bodyTemplate,
        defaultTargetBranch: override.defaultTargetBranch ?? base.defaultTargetBranch,
        targetRules: override.targetRules ?? base.targetRules,
      }
    }
  }
  return base
}
