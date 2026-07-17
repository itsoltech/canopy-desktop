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
  'projectOverrides are keyed by the tracker project key — the task-key prefix (GAKKO-1 → GAKKO) — and take precedence over the root templates for tasks from that project.',
  'Task data (key, title, type, URL) comes from the tracker(s) listed in `trackers`; the active task key is usually embedded in the current branch name (e.g. ABC-123).',
  'The user may explicitly override any of the above (branch name, PR title/description/target) when creating a branch or PR — an explicit user request always wins.',
  'You MUST NOT modify .canopy/config.json or any other Canopy configuration file — they are user-managed.',
]

export function defaultConfig(): RepoConfig {
  return {
    version: 1 as const,
    trackers: [],
    projectOverrides: {},
    filters: {
      assignedToMe: true,
      statuses: [],
    },
    agents: {
      instructions: DEFAULT_AGENT_INSTRUCTIONS,
    },
  }
}

/** Task-key prefix → tracker project key (`GAKKO-743` → `GAKKO`; GitHub's `#123` → undefined). */
export function projectKeyOfTask(taskKey: string | undefined): string | undefined {
  if (!taskKey) return undefined
  const m = /^([A-Za-z][A-Za-z0-9_]*)-\d/.exec(taskKey)
  return m ? m[1].toUpperCase() : undefined
}

export function getBranchTemplate(
  config: RepoConfig,
  projectKey?: string,
): BranchTemplateConfig & { typeMapping?: Record<string, string> } {
  const base = config.branchTemplate ?? DEFAULT_BRANCH_TEMPLATE
  if (projectKey) {
    const override = config.projectOverrides[projectKey.toUpperCase()]?.branchTemplate
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

export function getPRTemplate(config: RepoConfig, projectKey?: string): PRTemplateConfig {
  // The editor persists prTemplate per FIELD, so a stored object can be partial (e.g. only the
  // target branch set) — fall back to the built-in default field-wise, never as a whole.
  const stored = config.prTemplate
  const base: PRTemplateConfig = {
    titleTemplate: stored?.titleTemplate || DEFAULT_PR_TEMPLATE.titleTemplate,
    bodyTemplate: stored?.bodyTemplate || DEFAULT_PR_TEMPLATE.bodyTemplate,
    defaultTargetBranch: stored?.defaultTargetBranch || DEFAULT_PR_TEMPLATE.defaultTargetBranch,
    targetRules: stored?.targetRules ?? DEFAULT_PR_TEMPLATE.targetRules,
  }
  if (projectKey) {
    const override = config.projectOverrides[projectKey.toUpperCase()]?.prTemplate
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
