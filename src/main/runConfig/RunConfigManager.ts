import { readFile, writeFile, mkdir, readdir } from 'fs/promises'
import { join, relative, dirname, sep } from 'path'
import { ok, err, type ResultAsync } from 'neverthrow'
import { parse, stringify } from 'smol-toml'
import type { RunConfigFile, RunConfigSource, RunConfiguration } from './types'
import type { RunConfigError } from './errors'
import { fromExternalCall } from '../errors'
import { SAFETY_IGNORE_PATTERNS } from '../fileWatcher/defaults'

const CONFIG_DIR = '.canopy'
const CONFIG_FILE = 'run.toml'

function tomlPath(configDir: string): string {
  return join(configDir, CONFIG_DIR, CONFIG_FILE)
}

export class RunConfigManager {
  discover(repoRoot: string): ResultAsync<RunConfigSource[], RunConfigError> {
    return fromExternalCall(this.scanForConfigs(repoRoot), () => ({
      _tag: 'RunConfigNotFound' as const,
      path: repoRoot,
    }))
  }

  loadFile(configDir: string): ResultAsync<RunConfigFile, RunConfigError> {
    const path = tomlPath(configDir)
    return fromExternalCall(readFile(path, 'utf-8'), () => ({
      _tag: 'RunConfigNotFound' as const,
      path,
    })).andThen((raw) => {
      try {
        const parsed = parse(raw) as Record<string, unknown>
        const configurations = Array.isArray(parsed.configurations)
          ? (parsed.configurations as RunConfiguration[])
          : []
        return ok({ configurations })
      } catch (e) {
        return err({
          _tag: 'RunConfigParseError' as const,
          path,
          reason: e instanceof Error ? e.message : String(e),
        })
      }
    })
  }

  saveFile(configDir: string, config: RunConfigFile): ResultAsync<void, RunConfigError> {
    const path = tomlPath(configDir)
    return fromExternalCall(
      (async () => {
        await mkdir(join(configDir, CONFIG_DIR), { recursive: true })
        await writeFile(
          path,
          stringify(config as unknown as Record<string, unknown>) + '\n',
          'utf-8',
        )
      })(),
      (e) => ({
        _tag: 'RunConfigWriteError' as const,
        path,
        reason: e instanceof Error ? e.message : String(e),
      }),
    )
  }

  addConfiguration(
    configDir: string,
    configuration: RunConfiguration,
  ): ResultAsync<void, RunConfigError> {
    return this.loadFile(configDir)
      .orElse(() => ok<RunConfigFile, RunConfigError>({ configurations: [] }))
      .andThen((file) => {
        const exists = file.configurations.some((c) => c.name === configuration.name)
        if (exists) {
          return err({
            _tag: 'RunConfigValidationError' as const,
            name: configuration.name,
            reason: 'Configuration with this name already exists',
          })
        }
        file.configurations.push(configuration)
        return this.saveFile(configDir, file)
      })
  }

  updateConfiguration(
    configDir: string,
    oldName: string,
    configuration: RunConfiguration,
  ): ResultAsync<void, RunConfigError> {
    return this.loadFile(configDir).andThen((file) => {
      const idx = file.configurations.findIndex((c) => c.name === oldName)
      if (idx === -1) {
        return err({
          _tag: 'RunConfigNotFound' as const,
          path: tomlPath(configDir),
        })
      }
      file.configurations[idx] = configuration
      return this.saveFile(configDir, file)
    })
  }

  deleteConfiguration(configDir: string, name: string): ResultAsync<void, RunConfigError> {
    return this.loadFile(configDir).andThen((file) => {
      file.configurations = file.configurations.filter((c) => c.name !== name)
      return this.saveFile(configDir, file)
    })
  }

  private async scanForConfigs(repoRoot: string): Promise<RunConfigSource[]> {
    const ignoreSet = new Set(SAFETY_IGNORE_PATTERNS)
    const entries = await readdir(repoRoot, { recursive: true, withFileTypes: true })
    const sources: RunConfigSource[] = []

    for (const entry of entries) {
      if (!entry.isFile()) continue
      if (entry.name !== CONFIG_FILE) continue

      const parentPath = entry.parentPath ?? entry.path
      if (!parentPath.endsWith(CONFIG_DIR)) continue

      // Skip ignored directories
      const relFromRoot = relative(repoRoot, parentPath)
      const segments = relFromRoot.split(sep)
      if (segments.some((s) => ignoreSet.has(s))) continue

      const configDir = dirname(parentPath)
      const relativePath = relative(repoRoot, configDir)
      const filePath = join(parentPath, CONFIG_FILE)

      try {
        const raw = await readFile(filePath, 'utf-8')
        const parsed = parse(raw) as Record<string, unknown>
        const configurations = Array.isArray(parsed.configurations)
          ? (parsed.configurations as RunConfiguration[])
          : []
        sources.push({ configDir, relativePath: relativePath || '.', file: { configurations } })
      } catch {
        // Skip unparseable files
      }
    }

    return sources
  }
}
