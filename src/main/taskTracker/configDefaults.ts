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

/**
 * Written into `.canopy/config.json` for AI agents that read the file directly. Keep these
 * self-contained: an agent may see the config without any other Canopy context.
 */
export const DEFAULT_AGENT_INSTRUCTIONS: string[] = [
  'Branch names: render branchTemplate.template ({branchType}, {taskKey}, {taskTitle} slugified to lowercase-with-dashes); map the task type to {branchType} via branchTemplate.typeMapping.',
  'Pull requests: render the title from prTemplate.titleTemplate and the description from prTemplate.bodyTemplate ({taskKey}, {taskTitle}, {taskType}, {taskUrl}, {boardKey}).',
  'Target pull requests at prTemplate.defaultTargetBranch unless an entry in prTemplate.targetRules matches the task type.',
  'boardOverrides take precedence over the root templates for tasks from that board.',
  'Task data (key, title, type, URL) comes from the tracker(s) listed in `trackers`; the active task key is usually embedded in the current branch name (e.g. ABC-123).',
  'The user may explicitly override any of the above (branch name, PR title/description/target) when creating a branch or PR — an explicit user request always wins.',
  'You MUST NOT modify .canopy/config.json or any other Canopy configuration file — they are user-managed.',
]

export function defaultConfig(): RepoConfig {
  return {
    version: 1 as const,
    trackers: [],
    boardOverrides: {},
    filters: {
      assignedToMe: true,
      statuses: [],
    },
    agents: {
      instructions: DEFAULT_AGENT_INSTRUCTIONS,
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
  // The editor persists prTemplate per FIELD, so a stored object can be partial (e.g. only the
  // target branch set) — fall back to the built-in default field-wise, never as a whole.
  const stored = config.prTemplate
  const base: PRTemplateConfig = {
    titleTemplate: stored?.titleTemplate || DEFAULT_PR_TEMPLATE.titleTemplate,
    bodyTemplate: stored?.bodyTemplate || DEFAULT_PR_TEMPLATE.bodyTemplate,
    defaultTargetBranch: stored?.defaultTargetBranch || DEFAULT_PR_TEMPLATE.defaultTargetBranch,
    targetRules: stored?.targetRules ?? DEFAULT_PR_TEMPLATE.targetRules,
  }
  if (boardId) {
    const override = config.boardOverrides[boardId]?.prTemplate
    if (override) {
      return {
        titleTemplate: override.titleTemplate || base.titleTemplate,
        bodyTemplate: override.bodyTemplate || base.bodyTemplate,
        defaultTargetBranch: override.defaultTargetBranch || base.defaultTargetBranch,
        targetRules: override.targetRules ?? base.targetRules,
      }
    }
  }
  return base
}
