import { readFile, writeFile, mkdir, readdir } from 'fs/promises'
import { join, relative } from 'path'
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

/**
 * `run.toml` is git-shared and hand-editable, and the UI keys configurations by name. Drop
 * nameless entries and later duplicates (e.g. left by a merge) from what discovery shows, rather
 * than handing the renderer colliding keys. Edits still operate on the file as written.
 */
function uniqueNamedConfigurations(value: unknown): RunConfiguration[] {
  if (!Array.isArray(value)) return []
  const seen = new Set<string>()
  return value.filter((c): c is RunConfiguration => {
    if (typeof c !== 'object' || c === null || typeof c.name !== 'string' || !c.name) return false
    if (seen.has(c.name)) return false
    seen.add(c.name)
    return true
  })
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
          // smol-toml's stringify() accepts an untyped Record; bridge RunConfigFile
          // through `unknown` to satisfy the library's loose parameter type.
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
    const sources: RunConfigSource[] = []
    await this.walkDir(repoRoot, repoRoot, ignoreSet, sources)
    return sources
  }

  private async walkDir(
    dir: string,
    repoRoot: string,
    ignoreSet: Set<string>,
    sources: RunConfigSource[],
  ): Promise<void> {
    let entries
    try {
      entries = await readdir(dir, { withFileTypes: true })
    } catch {
      return
    }

    for (const entry of entries) {
      if (!entry.isDirectory()) continue
      if (ignoreSet.has(entry.name)) continue

      const fullPath = join(dir, entry.name)

      if (entry.name === CONFIG_DIR) {
        const filePath = join(fullPath, CONFIG_FILE)
        try {
          const raw = await readFile(filePath, 'utf-8')
          const parsed = parse(raw) as Record<string, unknown>
          const configurations = uniqueNamedConfigurations(parsed.configurations)
          const configDir = dir
          const relativePath = relative(repoRoot, configDir) || '.'
          sources.push({ configDir, relativePath, file: { configurations } })
        } catch {
          // No run.toml or unparseable — skip
        }
      } else {
        await this.walkDir(fullPath, repoRoot, ignoreSet, sources)
      }
    }
  }
}
