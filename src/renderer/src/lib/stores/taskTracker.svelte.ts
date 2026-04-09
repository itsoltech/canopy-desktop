import { setPref } from './preferences.svelte'

export interface ActiveTaskContext {
  taskKey: string
  summary: string
  connectionId: string
  boardId?: string
}

export interface TrackerCredentialState {
  hasToken: boolean
  username?: string
}

let connections: TaskTrackerConnectionInfo[] = $state([])
let loadCount = $state(0)
const loading = $derived(loadCount > 0)
let repoConfig: RepoConfig | null = $state(null)
let globalConfig: RepoConfig | null = $state(null)
let resolvedConfig: ResolvedConfig | null = $state(null)
let lastRepoRoot: string | undefined = $state(undefined)
// Per-tracker credentials: keyed by trackerId
let trackerCredentials = $state<Record<string, TrackerCredentialState>>({})
let activeTask: ActiveTaskContext | null = $state(null)

export function getTaskTrackerConnections(): TaskTrackerConnectionInfo[] {
  return connections
}

export function isTaskTrackerLoading(): boolean {
  return loading
}

export function getRepoConfig(): RepoConfig | null {
  return repoConfig
}

export function getGlobalConfig(): RepoConfig | null {
  return globalConfig
}

export function getResolvedConfig(): ResolvedConfig | null {
  return resolvedConfig
}

export function getTrackerCredentials(): Record<string, TrackerCredentialState> {
  return trackerCredentials
}

export function hasAnyCredentials(): boolean {
  return Object.values(trackerCredentials).some((c) => c.hasToken)
}

export function getTrackerCredential(trackerId: string): TrackerCredentialState | null {
  return trackerCredentials[trackerId] ?? null
}

async function refreshCredentials(trackers: TrackerConfig[]): Promise<void> {
  const entries = await Promise.all(
    trackers
      .filter((t) => t.baseUrl)
      .map(async (t) => {
        try {
          const has = await window.api.keychainHasCredentials(t.provider, t.baseUrl)
          if (has) {
            const info = await window.api.keychainGetCredentials(t.provider, t.baseUrl)
            return [t.id, { hasToken: true, username: info?.username }] as const
          }
          return [t.id, { hasToken: false }] as const
        } catch {
          return [t.id, { hasToken: false }] as const
        }
      }),
  )
  trackerCredentials = Object.fromEntries(entries)
}

export async function loadRepoConfig(repoRoot: string): Promise<void> {
  lastRepoRoot = repoRoot
  loadCount++
  try {
    repoConfig = await window.api.repoConfigLoad(repoRoot)
    resolvedConfig = await window.api.trackerResolvedConfig(repoRoot)
    if (resolvedConfig) {
      await refreshCredentials(resolvedConfig.config.trackers)
    }
  } catch {
    repoConfig = null
  } finally {
    loadCount--
  }
}

export async function saveRepoConfig(repoRoot: string, config: RepoConfig): Promise<void> {
  const plain = JSON.parse(JSON.stringify(config)) as RepoConfig
  await window.api.repoConfigSave(repoRoot, plain)
  repoConfig = plain
  resolvedConfig = await window.api.trackerResolvedConfig(repoRoot)
  if (resolvedConfig) {
    await refreshCredentials(resolvedConfig.config.trackers)
  }
}

export async function initRepoConfig(repoRoot: string): Promise<RepoConfig> {
  const config = await window.api.repoConfigInit(repoRoot)
  repoConfig = config
  resolvedConfig = await window.api.trackerResolvedConfig(repoRoot)
  return config
}

export async function loadGlobalConfig(): Promise<void> {
  loadCount++
  try {
    globalConfig = await window.api.globalConfigLoad()
    // Refresh credentials for all resolved trackers (global + repo) so
    // repo tracker credential state isn't wiped when called in isolation
    const resolved = await window.api.trackerResolvedConfig(lastRepoRoot)
    if (resolved) {
      resolvedConfig = resolved
      await refreshCredentials(resolved.config.trackers)
    } else if (globalConfig) {
      await refreshCredentials(globalConfig.trackers)
    }
  } catch {
    globalConfig = null
  } finally {
    loadCount--
  }
}

export async function saveGlobalConfig(config: RepoConfig): Promise<void> {
  const plain = JSON.parse(JSON.stringify(config)) as RepoConfig
  await window.api.globalConfigSave(plain)
  globalConfig = plain
  // Re-resolve merged config so sidebar reflects the change
  resolvedConfig = await window.api.trackerResolvedConfig(lastRepoRoot)
  const allTrackers = resolvedConfig?.config.trackers ?? plain.trackers
  await refreshCredentials(allTrackers)
}

export async function initGlobalConfig(): Promise<RepoConfig> {
  const config: RepoConfig = {
    version: 1,
    trackers: [],
    boardOverrides: {},
    filters: { assignedToMe: true, statuses: [] },
  }
  await window.api.globalConfigSave(config)
  globalConfig = config
  return config
}

// --- Active Task ---

export function getActiveTask(): ActiveTaskContext | null {
  return activeTask
}

export async function setActiveTask(worktreePath: string, task: ActiveTaskContext): Promise<void> {
  activeTask = task
  await setPref(`activeTask.${worktreePath}`, JSON.stringify(task))
}

export async function loadActiveTask(worktreePath: string): Promise<void> {
  const raw = await window.api.getPref(`activeTask.${worktreePath}`)
  if (raw) {
    try {
      activeTask = JSON.parse(raw) as ActiveTaskContext
    } catch {
      activeTask = null
    }
  } else {
    activeTask = null
  }
}

export async function clearActiveTask(worktreePath: string): Promise<void> {
  activeTask = null
  await setPref(`activeTask.${worktreePath}`, '')
}

export async function loadConnections(): Promise<void> {
  loadCount++
  try {
    connections = await window.api.taskTrackerGetConnections()
  } catch {
    connections = []
  } finally {
    loadCount--
  }
}

export async function removeConnection(connectionId: string): Promise<void> {
  await window.api.taskTrackerRemoveConnection(connectionId)
  connections = connections.filter((c) => c.id !== connectionId)
}
