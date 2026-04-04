import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs'
import { join } from 'path'
import { ok, err, type Result } from 'neverthrow'
import type { RepoConfig, BranchTemplateConfig, PRTemplateConfig } from './types'
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

const DEFAULT_BRANCH_TEMPLATE: BranchTemplateConfig & { typeMapping?: Record<string, string> } = {
  template: '{branchType}/{taskKey}-{taskTitle}',
  customVars: {},
}

const DEFAULT_PR_TEMPLATE: PRTemplateConfig = {
  titleTemplate: '[{taskKey}] {taskTitle}',
  bodyTemplate: '## {taskKey}: {taskTitle}\n\n{taskUrl}',
  defaultTargetBranch: '',
  targetRules: [],
}

function defaultConfig(): RepoConfig {
  return {
    version: CURRENT_VERSION as 1,
    tracker: {
      provider: 'jira',
      baseUrl: '',
    },
    boardOverrides: {},
    filters: {
      assignedToMe: true,
      statuses: [],
    },
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
      // Fill missing required fields from defaults (handles old config formats)
      const defaults = defaultConfig()
      const normalized: RepoConfig = {
        version: 1,
        tracker: parsed.tracker ?? defaults.tracker,
        branchTemplate: parsed.branchTemplate,
        prTemplate: parsed.prTemplate,
        boardOverrides: parsed.boardOverrides ?? defaults.boardOverrides,
        filters: parsed.filters ?? defaults.filters,
      }
      return ok(normalized)
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

  getPRTemplate(config: RepoConfig, boardId?: string): PRTemplateConfig {
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
}
