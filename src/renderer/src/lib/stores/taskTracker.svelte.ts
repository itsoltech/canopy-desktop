import { getPref, setPref } from './preferences.svelte'

export interface ActiveTaskContext {
  taskKey: string
  summary: string
  connectionId: string
  boardId?: string
}

let connections: TaskTrackerConnectionInfo[] = $state([])
let loading = $state(false)
let repoConfig: RepoConfig | null = $state(null)
let hasCredentials = $state(false)
let credentialsInfo: { username?: string; hasToken: boolean } | null = $state(null)
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

// --- Active Task ---

export function getActiveTask(): ActiveTaskContext | null {
  return activeTask
}

export async function setActiveTask(worktreePath: string, task: ActiveTaskContext): Promise<void> {
  activeTask = task
  await setPref(`activeTask.${worktreePath}`, JSON.stringify(task))
}

export function loadActiveTask(worktreePath: string): void {
  const raw = getPref(`activeTask.${worktreePath}`)
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
