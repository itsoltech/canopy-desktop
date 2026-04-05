import { readFile, writeFile, mkdir, access } from 'fs/promises'
import { join } from 'path'
import { ok, err, type ResultAsync } from 'neverthrow'
import type { RepoConfig, BranchTemplateConfig, PRTemplateConfig } from './types'
import type { TaskTrackerError } from './errors'
import { fromExternalCall } from '../errors'

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
  async exists(repoRoot: string): Promise<boolean> {
    try {
      await access(configPath(repoRoot))
      return true
    } catch {
      return false
    }
  }

  load(repoRoot: string): ResultAsync<RepoConfig, TaskTrackerError> {
    return fromExternalCall(readFile(configPath(repoRoot), 'utf-8'), () => ({
      _tag: 'ConfigNotFound' as const,
      repoRoot,
    })).andThen((raw) => {
      try {
        const parsed = JSON.parse(raw) as RepoConfig
        if (parsed.version !== CURRENT_VERSION) {
          return err({
            _tag: 'ConfigParseError' as const,
            repoRoot,
            reason: `Unsupported config version: ${String(parsed.version)}`,
          })
        }
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
          _tag: 'ConfigParseError' as const,
          repoRoot,
          reason: e instanceof Error ? e.message : String(e),
        })
      }
    })
  }

  save(repoRoot: string, config: RepoConfig): ResultAsync<void, TaskTrackerError> {
    return fromExternalCall(
      (async () => {
        const dir = configDir(repoRoot)
        await mkdir(dir, { recursive: true })
        await writeFile(configPath(repoRoot), JSON.stringify(config, null, 2) + '\n', 'utf-8')
      })(),
      (e) => ({
        _tag: 'ConfigWriteError' as const,
        repoRoot,
        reason: e instanceof Error ? e.message : String(e),
      }),
    )
  }

  init(repoRoot: string): ResultAsync<RepoConfig, TaskTrackerError> {
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
