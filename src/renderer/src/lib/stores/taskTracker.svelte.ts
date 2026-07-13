import { setPref } from './preferences.svelte'
import { trackersNeedingCredentials } from '../../components/preferences/_partials/configScopeLabels'

export interface ActiveTaskContext {
  taskKey: string
  summary: string
  connectionId: string
  boardId?: string
}

export interface TrackerCredentialState {
  hasToken: boolean
  username?: string
  /** false = the stored token was rejected by the tracker (expired/revoked); true = verified
   *  against the tracker API; undefined = not verified (e.g. offline or check still running). */
  valid?: boolean
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

/**
 * Trackers defined in the open repo's .canopy/config.json that have no stored credentials.
 * Drives the "needs credentials" dot + sidebar entry (project-file trackers only).
 */
export function getProjectTrackersNeedingCredentials(): TrackerConfig[] {
  return trackersNeedingCredentials(repoConfig?.trackers ?? [], (id) => {
    const c = trackerCredentials[id]
    // A token that the tracker rejected (expired/revoked) counts as missing credentials.
    return (c?.hasToken ?? false) && c?.valid !== false
  })
}

export function projectNeedsCredentials(): boolean {
  return getProjectTrackersNeedingCredentials().length > 0
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
            // Carry the verification flag over so frequent refreshes (e.g. template auto-saves)
            // don't wipe it; verifyCredentials re-runs only on config loads.
            return [
              t.id,
              { hasToken: true, username: info?.username, valid: trackerCredentials[t.id]?.valid },
            ] as const
          }
          return [t.id, { hasToken: false }] as const
        } catch {
          return [t.id, { hasToken: false }] as const
        }
      }),
  )
  trackerCredentials = Object.fromEntries(entries)
}

// Errors that mean the tracker rejected the token itself (vs. network being down etc.).
const AUTH_ERROR_RE = /\b(401|403)\b|unauthoriz|authenticat|forbidden|invalid token/i

/**
 * Verify stored tokens against the tracker API (lightweight getCurrentUser call) and flag the ones
 * the tracker rejects as `valid: false`. A token merely existing in the keychain says nothing about
 * whether it still works — expired/revoked tokens would otherwise show as "Connected" forever.
 * Non-auth failures (offline, DNS) leave `valid` undefined so we don't cry wolf.
 */
async function verifyCredentials(trackers: TrackerConfig[], repoRoot?: string): Promise<void> {
  await Promise.all(
    trackers
      .filter((t) => trackerCredentials[t.id]?.hasToken)
      .map(async (t) => {
        try {
          await window.api.trackerConfigGetCurrentUser(repoRoot, t.id)
          const cur = trackerCredentials[t.id]
          if (cur?.hasToken) trackerCredentials[t.id] = { ...cur, valid: true }
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e)
          const cur = trackerCredentials[t.id]
          if (AUTH_ERROR_RE.test(msg) && cur?.hasToken) {
            trackerCredentials[t.id] = { ...cur, valid: false }
          }
        }
      }),
  )
}

export async function loadRepoConfig(repoRoot: string): Promise<void> {
  lastRepoRoot = repoRoot
  loadCount++
  try {
    repoConfig = await window.api.repoConfigLoad(repoRoot)
    resolvedConfig = await window.api.trackerResolvedConfig(repoRoot)
    if (resolvedConfig) {
      await refreshCredentials(resolvedConfig.config.trackers)
      // Fire-and-forget: validity flags arrive asynchronously so config load isn't blocked on API calls.
      void verifyCredentials(resolvedConfig.config.trackers, repoRoot)
    }
  } catch {
    repoConfig = null
  } finally {
    loadCount--
  }
}

export async function saveRepoConfig(repoRoot: string, config: RepoConfig): Promise<void> {
  const plain = $state.snapshot(config) as RepoConfig
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
      void verifyCredentials(resolved.config.trackers, lastRepoRoot)
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
  const plain = $state.snapshot(config) as RepoConfig
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

export async function loadActiveTask(
  worktreePath: string,
  options: { shouldApply?: () => boolean } = {},
): Promise<void> {
  const raw = await window.api.getPref(`activeTask.${worktreePath}`)
  if (options.shouldApply && !options.shouldApply()) return
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
