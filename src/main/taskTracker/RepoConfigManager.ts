import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs'
import { join } from 'path'
import { ok, err, type Result } from 'neverthrow'
import type { RepoConfig, ProjectConfig, BranchTemplateConfig, PRTemplateConfig } from './types'
import type { TaskTrackerError } from './errors'

const CONFIG_DIR = '.canopy'
const CONFIG_FILE = 'config.json'
const CURRENT_VERSION = 1

function configDir(repoRoot: string): string {
  return join(repoRoot, CONFIG_DIR)
}

function configPath(repoRoot: string): string {
  return join(repoRoot, CONFIG_DIR, CONFIG_FILE)
}

function defaultConfig(): RepoConfig {
  return {
    version: CURRENT_VERSION as 1,
    tracker: {
      provider: 'jira',
      baseUrl: '',
    },
    projects: {},
    filters: {
      assignedToMe: true,
      statuses: [],
    },
  }
}

export function defaultProjectConfig(): ProjectConfig {
  return {
    branchTemplate: {
      template: '{branchType}/{taskKey}-{taskTitle}',
      customVars: {},
    },
    prTemplate: {
      titleTemplate: '[{taskKey}] {taskTitle}',
      bodyTemplate: '## {taskKey}: {taskTitle}\n\n{taskUrl}',
      defaultTargetBranch: '',
      targetRules: [],
    },
    boardOverrides: {},
  }
}

export class RepoConfigManager {
  exists(repoRoot: string): boolean {
    return existsSync(configPath(repoRoot))
  }

  load(repoRoot: string): Result<RepoConfig, TaskTrackerError> {
    const path = configPath(repoRoot)
    if (!existsSync(path)) {
      return err({ _tag: 'ConfigNotFound', repoRoot })
    }
    try {
      const raw = readFileSync(path, 'utf-8')
      const parsed = JSON.parse(raw) as RepoConfig
      if (parsed.version !== CURRENT_VERSION) {
        return err({
          _tag: 'ConfigParseError',
          repoRoot,
          reason: `Unsupported config version: ${String(parsed.version)}`,
        })
      }
      return ok(parsed)
    } catch (e) {
      return err({
        _tag: 'ConfigParseError',
        repoRoot,
        reason: e instanceof Error ? e.message : String(e),
      })
    }
  }

  save(repoRoot: string, config: RepoConfig): Result<void, TaskTrackerError> {
    try {
      const dir = configDir(repoRoot)
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true })
      }
      writeFileSync(configPath(repoRoot), JSON.stringify(config, null, 2) + '\n', 'utf-8')
      return ok(undefined)
    } catch (e) {
      return err({
        _tag: 'ConfigWriteError',
        repoRoot,
        reason: e instanceof Error ? e.message : String(e),
      })
    }
  }

  init(repoRoot: string): Result<RepoConfig, TaskTrackerError> {
    const config = defaultConfig()
    return this.save(repoRoot, config).map(() => config)
  }

  getBranchTemplate(
    config: RepoConfig,
    projectKey: string,
    boardId?: string,
  ): BranchTemplateConfig & { typeMapping?: Record<string, string> } {
    const project = config.projects[projectKey]
    if (!project) return defaultProjectConfig().branchTemplate

    if (boardId) {
      const override = project.boardOverrides[boardId]?.branchTemplate
      if (override) {
        return {
          template: override.template ?? project.branchTemplate.template,
          customVars: { ...project.branchTemplate.customVars, ...override.customVars },
          typeMapping: override.typeMapping ?? project.branchTemplate.typeMapping,
        }
      }
    }
    return project.branchTemplate
  }

  getPRTemplate(config: RepoConfig, projectKey: string, boardId?: string): PRTemplateConfig {
    const project = config.projects[projectKey]
    if (!project) return defaultProjectConfig().prTemplate

    if (boardId) {
      const override = project.boardOverrides[boardId]?.prTemplate
      if (override) {
        return {
          titleTemplate: override.titleTemplate ?? project.prTemplate.titleTemplate,
          bodyTemplate: override.bodyTemplate ?? project.prTemplate.bodyTemplate,
          defaultTargetBranch:
            override.defaultTargetBranch ?? project.prTemplate.defaultTargetBranch,
          targetRules: override.targetRules ?? project.prTemplate.targetRules,
        }
      }
    }
    return project.prTemplate
  }
}
