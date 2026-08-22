import { randomUUID } from 'crypto'
import { access, mkdir, readFile, rename, unlink, writeFile } from 'fs/promises'
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

export class RepoConfigManager {
  /**
   * Only a genuine ENOENT counts as "absent". A file that is there but cannot be
   * reached (EACCES on the directory, a transient EMFILE) reports `true`, because
   * both callers treat `false` as permission to act as if nothing were there —
   * `performConfigWrite` initializes defaults over it, and binding-pruning drops
   * it from the live set. Guessing "absent" from an unreadable path would destroy
   * a config or unbind a credential the repository still uses.
   */
  async exists(repoRoot: string): Promise<boolean> {
    try {
      await access(configPath(repoRoot))
      return true
    } catch (error) {
      return (error as NodeJS.ErrnoException)?.code !== 'ENOENT'
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
        let trackers = (parsed as Record<string, unknown>).trackers as
          RepoConfig['trackers'] | undefined
        const VALID_PROVIDERS = new Set(['jira', 'youtrack', 'github'])
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
          // Must survive the load→save round-trip: normalization drops unknown fields, so
          // omitting `ci` here would erase the user's hand-edited CI block on the next save.
          // Keep the RAW value — `parseCiConfig` is applied at read time by CiManager, so a
          // block that fails validation reads as `CiConfigInvalid` (distinct from "not
          // configured") without being deleted from the user's (git-tracked) config file.
          ci: parsed.ci ?? undefined,
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
        const destination = configPath(repoRoot)
        const temporary = join(dir, `.${CONFIG_FILE}.${randomUUID()}.tmp`)
        let published = false
        try {
          // Same-directory rename is atomic: readers see either the previous complete JSON or
          // the new complete JSON, never the partially-written bytes of an in-place write.
          await writeFile(temporary, JSON.stringify(config, null, 2) + '\n', {
            encoding: 'utf-8',
            flag: 'wx',
          })
          await rename(temporary, destination)
          published = true
        } finally {
          if (!published) await unlink(temporary).catch(() => undefined)
        }
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
