let connections: TaskTrackerConnectionInfo[] = $state([])
let loading = $state(false)
let repoConfig: RepoConfig | null = $state(null)
let hasCredentials = $state(false)
let credentialsInfo: { username?: string; hasToken: boolean } | null = $state(null)

export function getTaskTrackerConnections(): TaskTrackerConnectionInfo[] {
  return connections
}

export function isTaskTrackerLoading(): boolean {
  return loading
}

export function getRepoConfig(): RepoConfig | null {
  return repoConfig
}

export function getHasCredentials(): boolean {
  return hasCredentials
}

export function getCredentialsInfo(): { username?: string; hasToken: boolean } | null {
  return credentialsInfo
}

export async function loadRepoConfig(repoRoot: string): Promise<void> {
  loading = true
  try {
    repoConfig = await window.api.repoConfigLoad(repoRoot)
    if (repoConfig) {
      hasCredentials = await window.api.keychainHasCredentials(
        repoConfig.tracker.provider,
        repoConfig.tracker.baseUrl,
      )
      credentialsInfo = await window.api.keychainGetCredentials(
        repoConfig.tracker.provider,
        repoConfig.tracker.baseUrl,
      )
    } else {
      hasCredentials = false
      credentialsInfo = null
    }
  } catch {
    repoConfig = null
    hasCredentials = false
    credentialsInfo = null
  } finally {
    loading = false
  }
}

export async function saveRepoConfig(repoRoot: string, config: RepoConfig): Promise<void> {
  // Strip Svelte proxies before sending through IPC (structured clone)
  const plain = JSON.parse(JSON.stringify(config)) as RepoConfig
  await window.api.repoConfigSave(repoRoot, plain)
  repoConfig = plain
}

export async function initRepoConfig(repoRoot: string): Promise<RepoConfig> {
  const config = await window.api.repoConfigInit(repoRoot)
  repoConfig = config
  return config
}

export async function loadConnections(): Promise<void> {
  loading = true
  try {
    connections = await window.api.taskTrackerGetConnections()
  } catch {
    connections = []
  } finally {
    loading = false
  }
}

export async function removeConnection(connectionId: string): Promise<void> {
  await window.api.taskTrackerRemoveConnection(connectionId)
  connections = connections.filter((c) => c.id !== connectionId)
}
