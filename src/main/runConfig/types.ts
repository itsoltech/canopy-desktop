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

export interface RunConfigFile {
  configurations: RunConfiguration[]
}

export interface RunConfigSource {
  configDir: string
  relativePath: string
  file: RunConfigFile
}
