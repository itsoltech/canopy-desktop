import { setPref } from './preferences.svelte'
import { trackersNeedingCredentials } from '../../components/preferences/_partials/configScopeLabels'
import { extractTaskKeys } from '../taskTracker/branchTaskKey'

export interface ActiveTaskContext {
  taskKey: string
  summary: string
  connectionId: string
  boardId?: string
}

/** Task backing the current worktree for the Task panel: the persisted activeTask, or a task
 *  resolved from the branch name (worktrees created outside Canopy). */
export interface PanelTaskContext extends ActiveTaskContext {
  source: 'active' | 'branch'
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

// Verification runs fire-and-forget after config loads; the UI shows a "checking credentials"
// hint instead of having the expired-credentials banner pop in unannounced seconds later.
let verifyCount = $state(0)

export function isVerifyingCredentials(): boolean {
  return verifyCount > 0
}

/**
 * Verify stored tokens against the tracker API (lightweight getCurrentUser call) and flag the ones
 * the tracker rejects as `valid: false`. A token merely existing in the keychain says nothing about
 * whether it still works — expired/revoked tokens would otherwise show as "Connected" forever.
 * Non-auth failures (offline, DNS) leave `valid` undefined so we don't cry wolf.
 */
async function verifyCredentials(trackers: TrackerConfig[], repoRoot?: string): Promise<void> {
  verifyCount++
  try {
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
  } finally {
    verifyCount--
  }
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

// Worktree paths reach this store with either separator (main returns backslashes on Windows, the
// sidebar uses forward slashes) — normalize so the writer and the reader agree on the pref key.
function activeTaskKey(worktreePath: string): string {
  return `activeTask.${worktreePath.replace(/\\/g, '/')}`
}

export function getActiveTask(): ActiveTaskContext | null {
  return activeTask
}

export async function setActiveTask(worktreePath: string, task: ActiveTaskContext): Promise<void> {
  activeTask = task
  await setPref(activeTaskKey(worktreePath), JSON.stringify(task))
}

export async function loadActiveTask(
  worktreePath: string,
  options: { shouldApply?: () => boolean } = {},
): Promise<void> {
  const raw = await window.api.getPref(activeTaskKey(worktreePath))
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
  await setPref(activeTaskKey(worktreePath), '')
}

// --- Panel tasks (worktree → task resolution for the Task inspector tab) ---

// A branch can reference several tasks (parent/subtask conventions) — all of them are tracked,
// with one selected for display in the panel.
let panelTasks = $state<PanelTaskContext[]>([])
let panelTaskIndex = $state(0)
// Worktree path the current panelTasks were resolved FOR. Resolution runs at the end of worktree
// hydration, so comparing this against the currently selected worktree tells the panel "these are
// still the previous worktree's tasks — show a loader instead".
let panelTaskPath = $state<string | null>(null)

export function getPanelTask(): PanelTaskContext | null {
  return panelTasks[panelTaskIndex] ?? null
}

export function getPanelTasks(): PanelTaskContext[] {
  return panelTasks
}

export function selectPanelTask(taskKey: string): void {
  const i = panelTasks.findIndex((t) => t.taskKey === taskKey)
  if (i >= 0) panelTaskIndex = i
}

export function getPanelTaskResolvedPath(): string | null {
  return panelTaskPath
}

/**
 * Resolve the tasks backing the current worktree: every task key found in the branch name,
 * validated against the tracker (keys the tracker doesn't know are dropped; bare keys are kept
 * when the tracker is unreachable), plus the persisted activeTask (written at branch creation).
 * The activeTask is selected when present; otherwise the last branch key wins — parent/subtask
 * branches name the parent first, and work happens on the most specific task.
 */
export async function resolvePanelTask(
  worktreePath: string,
  branch: string | null,
  options: { shouldApply?: () => boolean } = {},
): Promise<void> {
  const apply = (): boolean => !options.shouldApply || options.shouldApply()
  const normPath = worktreePath.replace(/\\/g, '/')
  const trackerId = resolvedConfig?.config.trackers[0]?.id ?? ''

  const keys = branch ? extractTaskKeys(branch) : []
  if (activeTask && !keys.includes(activeTask.taskKey)) keys.unshift(activeTask.taskKey)

  if (keys.length === 0) {
    if (apply()) {
      panelTasks = []
      panelTaskIndex = 0
      panelTaskPath = normPath
    }
    return
  }

  const contexts = await Promise.all(
    keys.map(async (key): Promise<PanelTaskContext | null> => {
      if (activeTask?.taskKey === key) return { ...activeTask, source: 'active' }
      try {
        const task = await window.api.trackerConfigFindTaskByKey(worktreePath, key)
        // The tracker answered and doesn't know this key — a false match, drop it.
        if (!task) return null
        return {
          taskKey: task.key,
          summary: task.summary,
          connectionId: trackerId,
          source: 'branch',
        }
      } catch {
        // Offline / expired credentials: keep the bare key so the panel can still render it.
        return { taskKey: key, summary: '', connectionId: trackerId, source: 'branch' }
      }
    }),
  )

  if (!apply()) return
  const list = contexts.filter((c): c is PanelTaskContext => c !== null)
  panelTasks = list
  const activeIdx = list.findIndex((c) => c.source === 'active')
  panelTaskIndex = activeIdx >= 0 ? activeIdx : Math.max(0, list.length - 1)
  panelTaskPath = normPath
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
