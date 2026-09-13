export interface RunConfiguration {
  name: string
  command: string
  args?: string
  cwd?: string
  env?: Record<string, string>
  max_instances?: number
  pre_run?: string
  post_run?: string
}

/**
 * `.canopy/run.toml` is committed to the repo, so its parsed contents are
 * untrusted: `Array.isArray` says nothing about element shape, and `command` /
 * `args` are interpolated into a shell command string downstream. Drop any
 * entry whose required string fields are not actually strings.
 */
export function isRunConfiguration(value: unknown): value is RunConfiguration {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  if (typeof candidate.name !== 'string' || typeof candidate.command !== 'string') return false
  for (const key of ['args', 'cwd', 'pre_run', 'post_run'] as const) {
    if (candidate[key] !== undefined && typeof candidate[key] !== 'string') return false
  }
  if (candidate.max_instances !== undefined && typeof candidate.max_instances !== 'number') {
    return false
  }
  return true
}

export interface RunConfigFile {
  configurations: RunConfiguration[]
}

export interface RunConfigSource {
  configDir: string
  relativePath: string
  file: RunConfigFile
}
