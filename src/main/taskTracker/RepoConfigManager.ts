import { readFile, writeFile, mkdir, access } from 'fs/promises'
import { join } from 'path'
import { ok, err, type ResultAsync } from 'neverthrow'
import type { RepoConfig, BranchTemplateConfig, PRTemplateConfig } from './types'
import type { TaskTrackerError } from './errors'
import { fromExternalCall } from '../errors'
import { defaultConfig, getBranchTemplate, getPRTemplate } from './configDefaults'

const CONFIG_DIR = '.canopy'
const CONFIG_FILE = 'config.json'
const CURRENT_VERSION = 1

function configDir(repoRoot: string): string {
  return join(repoRoot, CONFIG_DIR)
}

function configPath(repoRoot: string): string {
  return join(repoRoot, CONFIG_DIR, CONFIG_FILE)
}

const VALID_PROVIDERS = new Set(['jira', 'youtrack', 'github'])

/**
 * `.canopy/config.json` is checked into the repository, so a clone can carry
 * arbitrary contents. Drop tracker entries whose provider has no client:
 * createProviderClient() returns undefined for an unknown provider, which
 * surfaces as a raw TypeError deep inside a ResultAsync chain rather than a
 * typed TaskTrackerError. Returns undefined when the field isn't an array so
 * the legacy single-`tracker` fallback below still applies.
 */
function validTrackers(raw: unknown): RepoConfig['trackers'] | undefined {
  if (!Array.isArray(raw)) return undefined
  return raw.filter(
    (t): t is RepoConfig['trackers'][number] =>
      typeof t === 'object' &&
      t !== null &&
      VALID_PROVIDERS.has(String((t as { provider?: unknown }).provider)),
  )
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
        const parsed = JSON.parse(raw) as Record<string, unknown>
        if (parsed.version !== CURRENT_VERSION) {
          return err({
            _tag: 'ConfigParseError' as const,
            repoRoot,
            reason: `Unsupported config version: ${String(parsed.version)}`,
          })
        }
        const defaults = defaultConfig()

        // Backward compat: convert old single `tracker` to `trackers` array
        let trackers = validTrackers((parsed as Record<string, unknown>).trackers)
        if (!trackers && (parsed as Record<string, unknown>).tracker) {
          const old = (parsed as Record<string, unknown>).tracker as {
            provider: string
            baseUrl: string
          }
          if (!VALID_PROVIDERS.has(old.provider)) {
            return err({
              _tag: 'ConfigParseError' as const,
              repoRoot,
              reason: `Unknown provider: ${old.provider}`,
            })
          }
          trackers = [
            {
              id: `${old.provider}-default`,
              provider: old.provider as RepoConfig['trackers'][0]['provider'],
              baseUrl: old.baseUrl,
            },
          ]
        }

        const normalized: RepoConfig = {
          version: 1,
          trackers: trackers ?? defaults.trackers,
          branchTemplate: parsed.branchTemplate as RepoConfig['branchTemplate'],
          prTemplate: parsed.prTemplate as RepoConfig['prTemplate'],
          // Legacy `boardOverrides` are intentionally dropped — overrides are keyed by the
          // tracker PROJECT key (task-key prefix) now.
          projectOverrides: (parsed.projectOverrides ??
            defaults.projectOverrides) as RepoConfig['projectOverrides'],
          filters: (parsed.filters ?? defaults.filters) as RepoConfig['filters'],
          // Older files gain the default agent guidance on their next save.
          agents: (parsed.agents as RepoConfig['agents']) ?? defaults.agents,
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
    return getBranchTemplate(config, boardId)
  }

  getPRTemplate(config: RepoConfig, boardId?: string): PRTemplateConfig {
    return getPRTemplate(config, boardId)
  }
}
