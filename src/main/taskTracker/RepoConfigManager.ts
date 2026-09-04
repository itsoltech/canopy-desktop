import { randomUUID } from 'crypto'
import { access, link, mkdir, open, readdir, rename, stat, unlink, writeFile } from 'fs/promises'
import { join } from 'path'
import { setTimeout as delay } from 'timers/promises'
import { ok, err, type ResultAsync } from 'neverthrow'
import type { RepoConfig, BranchTemplateConfig, PRTemplateConfig } from './types'
import type { TaskTrackerError } from './errors'
import { fromExternalCall } from '../errors'
import { defaultConfig, getBranchTemplate, getPRTemplate } from './configDefaults'

const CONFIG_DIR = '.canopy'
const CONFIG_FILE = 'config.json'
const CURRENT_VERSION = 1
const CONFIG_TEMP_PREFIX = `.${CONFIG_FILE}.`
const CONFIG_TEMP_SUFFIX = '.tmp'
const STALE_CONFIG_TEMP_AGE_MS = 24 * 60 * 60 * 1_000
const RENAME_RETRY_DELAYS_MS = [25, 50, 100, 200] as const
const MAX_CONFIG_BYTES = 1024 * 1024

class ConfigTooLargeError extends Error {}

function serializeConfig(config: RepoConfig): string {
  const serialized = JSON.stringify(config, null, 2) + '\n'
  if (Buffer.byteLength(serialized, 'utf8') > MAX_CONFIG_BYTES) {
    throw new ConfigTooLargeError('Configuration file exceeds the 1 MiB size limit')
  }
  return serialized
}

async function readBoundedConfig(path: string): Promise<string> {
  const handle = await open(path, 'r')
  try {
    // Read no more than one byte past the limit. A pre-read stat alone would leave a
    // replacement/growing-file race and could still allocate an unbounded buffer.
    const buffer = Buffer.alloc(MAX_CONFIG_BYTES + 1)
    let offset = 0
    while (offset < buffer.length) {
      const { bytesRead } = await handle.read(buffer, offset, buffer.length - offset, offset)
      if (bytesRead === 0) break
      offset += bytesRead
    }
    if (offset > MAX_CONFIG_BYTES) {
      throw new ConfigTooLargeError('Configuration file exceeds the 1 MiB size limit')
    }
    return buffer.toString('utf8', 0, offset)
  } finally {
    await handle.close()
  }
}

function configDir(repoRoot: string): string {
  return join(repoRoot, CONFIG_DIR)
}

function configPath(repoRoot: string): string {
  return join(repoRoot, CONFIG_DIR, CONFIG_FILE)
}

function isTransientRenameError(error: unknown): boolean {
  const code = (error as NodeJS.ErrnoException)?.code
  return code === 'EPERM' || code === 'EACCES' || code === 'EBUSY'
}

function renameWithRetry(source: string, destination: string, attempt = 0): Promise<void> {
  return rename(source, destination).catch((error: unknown) => {
    const retryDelay = RENAME_RETRY_DELAYS_MS[attempt]
    if (!isTransientRenameError(error) || retryDelay === undefined) {
      return Promise.reject(error)
    }
    return delay(retryDelay).then(() => renameWithRetry(source, destination, attempt + 1))
  })
}

async function cleanupStaleConfigTemps(dir: string): Promise<void> {
  const entries = await readdir(dir, { withFileTypes: true }).catch(() => [])
  const staleBefore = Date.now() - STALE_CONFIG_TEMP_AGE_MS
  await Promise.all(
    entries
      .filter(
        (entry) =>
          entry.isFile() &&
          entry.name.startsWith(CONFIG_TEMP_PREFIX) &&
          entry.name.endsWith(CONFIG_TEMP_SUFFIX),
      )
      .map(async (entry) => {
        const candidate = join(dir, entry.name)
        const metadata = await stat(candidate).catch(() => null)
        if (metadata && metadata.mtimeMs <= staleBefore) {
          await unlink(candidate).catch(() => undefined)
        }
      }),
  )
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
    return fromExternalCall(readBoundedConfig(configPath(repoRoot)), (error) => {
      if (error instanceof ConfigTooLargeError) {
        return {
          _tag: 'ConfigParseError' as const,
          repoRoot,
          reason: error.message,
        }
      }
      if ((error as NodeJS.ErrnoException)?.code === 'ENOENT') {
        return { _tag: 'ConfigNotFound' as const, repoRoot }
      }
      return {
        _tag: 'ConfigReadError' as const,
        repoRoot,
        reason: error instanceof Error ? error.message : String(error),
      }
    }).andThen((raw) => {
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
        // Enforce the same limit on the exact bytes that load() will later read. Pretty-printing
        // can expand a compact, readable config beyond the cap, so check before touching disk.
        const serialized = serializeConfig(config)
        const dir = configDir(repoRoot)
        await mkdir(dir, { recursive: true })
        // A hard kill cannot run `finally`; remove old orphaned publications on a later save.
        // The age threshold avoids racing another live Canopy process writing the same repo.
        await cleanupStaleConfigTemps(dir)
        const destination = configPath(repoRoot)
        const temporary = join(dir, `.${CONFIG_FILE}.${randomUUID()}.tmp`)
        let published = false
        try {
          // Same-directory rename makes publication atomic for readers: they see either the
          // previous complete JSON or the new one. This does not claim fsync-level durability
          // across power loss; the goal here is to prevent consumers from parsing partial bytes.
          await writeFile(temporary, serialized, {
            encoding: 'utf-8',
            flag: 'wx',
          })
          // Windows may transiently reject replacement while an editor, AV or sync client holds
          // the destination. Bounded retries preserve atomic publication without falling back to
          // a partial in-place write.
          await renameWithRetry(temporary, destination)
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
    return fromExternalCall(
      (async () => {
        const serialized = serializeConfig(config)
        const dir = configDir(repoRoot)
        await mkdir(dir, { recursive: true })
        await cleanupStaleConfigTemps(dir)
        const temporary = join(dir, `.${CONFIG_FILE}.${randomUUID()}.tmp`)
        try {
          await writeFile(temporary, serialized, { encoding: 'utf-8', flag: 'wx' })
          // A same-directory hard link atomically publishes the already-complete bytes and fails
          // with EEXIST instead of replacing a config created by another process.
          await link(temporary, configPath(repoRoot))
          return config
        } finally {
          await unlink(temporary).catch(() => undefined)
        }
      })(),
      (e) => ({
        _tag: 'ConfigWriteError' as const,
        repoRoot,
        reason: e instanceof Error ? e.message : String(e),
      }),
    )
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
