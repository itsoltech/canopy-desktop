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
 *  resolved from the branch name (worktrees created outside Canopy). Status fields are filled
 *  from the tracker during resolution (absent when offline). */
export interface PanelTaskContext extends ActiveTaskContext {
  source: 'active' | 'branch'
  /** The tracker answered but doesn't know this key — deleted or invisible task. */
  missing?: boolean
  status?: string
  statusCategory?: string
  /** Normalized type (task/bug/…) and the tracker's own type name, when resolution succeeded. */
  type?: string
  typeName?: string
  /** Task-type icon as a data: URL (proxied — tracker icon URLs are authenticated). */
  typeIcon?: string
}

export interface TrackerCredentialState {
  hasToken: boolean
  username?: string
  credentialId?: string
  intendedUses?: string[]
  capabilities?: string[]
  verification?: Record<string, { state: string; checkedAt: string; reason?: string }>
  bindings?: string[]
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
let activeTasks = $state<ActiveTaskContext[]>([])

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

async function computeCredentials(
  trackers: TrackerConfig[],
  repoRoot?: string,
): Promise<Record<string, TrackerCredentialState>> {
  const entries = await Promise.all(
    trackers
      .filter((t) => t.baseUrl)
      .map(async (t) => {
        try {
          const bindingKey = `tracker:${t.id}`
          const has = await window.api.keychainHasCredentials(
            t.provider,
            t.baseUrl,
            bindingKey,
            repoRoot,
          )
          if (has) {
            const info = await window.api.keychainGetCredentials(
              t.provider,
              t.baseUrl,
              bindingKey,
              repoRoot,
            )
            // Carry the verification flag over so frequent refreshes (e.g. template auto-saves)
            // don't wipe it; verifyCredentials re-runs only on config loads.
            return [
              t.id,
              {
                hasToken: true,
                username: info?.username,
                credentialId: info?.credentialId,
                intendedUses: info?.intendedUses,
                capabilities: info?.capabilities,
                verification: info?.verification,
                bindings: info?.bindings,
                valid: trackerCredentials[t.id]?.valid,
              },
            ] as const
          }
          return [t.id, { hasToken: false }] as const
        } catch {
          return [t.id, { hasToken: false }] as const
        }
      }),
  )
  return Object.fromEntries(entries)
}

async function refreshCredentials(trackers: TrackerConfig[], repoRoot?: string): Promise<void> {
  trackerCredentials = await computeCredentials(trackers, repoRoot)
}

// Errors that mean the tracker rejected the token itself (vs. network being down etc.).
// 403 is capability/resource-specific (the token may still be valid for other integrations).
// Only authentication failures invalidate the whole credential.
const AUTH_ERROR_RE = /\b401\b|unauthoriz|authenticat|invalid token/i

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
async function verifyCredentials(
  trackers: TrackerConfig[],
  repoRoot?: string,
  shouldApply: () => boolean = () => true,
): Promise<void> {
  verifyCount++
  try {
    await Promise.all(
      trackers
        .filter((t) => trackerCredentials[t.id]?.hasToken)
        .map(async (t) => {
          try {
            await window.api.trackerConfigGetCurrentUser(repoRoot, t.id)
            if (!shouldApply()) return
            const cur = trackerCredentials[t.id]
            if (cur?.hasToken) trackerCredentials[t.id] = { ...cur, valid: true }
          } catch (e) {
            if (!shouldApply()) return
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

export async function loadRepoConfig(
  repoRoot: string,
  shouldApply: () => boolean = () => true,
): Promise<void> {
  loadCount++
  try {
    // Everything loads into locals first — a slow load for worktree A must not be able to
    // overwrite the state that a faster switch to worktree B has already applied.
    const nextRepo = await window.api.repoConfigLoad(repoRoot)
    const nextResolved = await window.api.trackerResolvedConfig(repoRoot)
    const nextCredentials = nextResolved
      ? await computeCredentials(nextResolved.config.trackers, repoRoot)
      : {}
    if (!shouldApply()) return
    lastRepoRoot = repoRoot
    repoConfig = nextRepo
    resolvedConfig = nextResolved
    trackerCredentials = nextCredentials
    if (nextResolved) {
      // Fire-and-forget: validity flags arrive asynchronously so config load isn't blocked on
      // API calls — and they re-check the generation before writing.
      void verifyCredentials(nextResolved.config.trackers, repoRoot, shouldApply)
    }
  } catch {
    if (!shouldApply()) return
    // The CURRENT worktree failed to load — leaving the previous worktree's trackers visible
    // would let a later save target the wrong project config.
    lastRepoRoot = repoRoot
    repoConfig = null
    resolvedConfig = null
    trackerCredentials = {}
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
    await refreshCredentials(resolvedConfig.config.trackers, repoRoot)
  }
}

export async function initRepoConfig(repoRoot: string): Promise<RepoConfig> {
  const config = await window.api.repoConfigInit(repoRoot)
  repoConfig = config
  resolvedConfig = await window.api.trackerResolvedConfig(repoRoot)
  return config
}

// The merged view drops personal trackers that duplicate a repo tracker (same provider + URL),
// but Settings still lists them under their own ids — refresh credentials for both.
function allKnownTrackers(resolved: ResolvedConfig | null): TrackerConfig[] {
  const result: TrackerConfig[] = [...(resolved?.config.trackers ?? [])]
  for (const t of globalConfig?.trackers ?? []) {
    if (!result.some((r) => r.id === t.id)) result.push(t)
  }
  return result
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
      const trackers = allKnownTrackers(resolved)
      await refreshCredentials(trackers, lastRepoRoot)
      void verifyCredentials(trackers, lastRepoRoot)
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
  await refreshCredentials(allKnownTrackers(resolvedConfig), lastRepoRoot)
}

export async function initGlobalConfig(): Promise<RepoConfig> {
  const config: RepoConfig = {
    version: 1,
    trackers: [],
    projectOverrides: {},
    filters: { assignedToMe: true, statuses: [] },
  }
  await window.api.globalConfigSave(config)
  globalConfig = config
  return config
}

// --- Active Tasks (explicitly linked to a worktree, persisted per worktree path) ---

// Worktree paths reach this store with either separator (main returns backslashes on Windows, the
// sidebar uses forward slashes) — normalize so the writer and the reader agree on the pref key.
function activeTaskKey(worktreePath: string): string {
  return `activeTask.${worktreePath.replace(/\\/g, '/')}`
}

export function getActiveTasks(): ActiveTaskContext[] {
  return activeTasks
}

async function persistActiveTasks(worktreePath: string): Promise<void> {
  await setPref(
    activeTaskKey(worktreePath),
    activeTasks.length > 0 ? JSON.stringify(activeTasks) : '',
  )
}

/** Replace the linked tasks with a single one — used right after creating a worktree from a task. */
export async function setActiveTask(worktreePath: string, task: ActiveTaskContext): Promise<void> {
  activeTasks = [task]
  await persistActiveTasks(worktreePath)
}

/** Link an additional task to the worktree (no-op when already linked). */
export async function addActiveTask(worktreePath: string, task: ActiveTaskContext): Promise<void> {
  if (!activeTasks.some((t) => t.taskKey === task.taskKey)) {
    activeTasks = [...activeTasks, task]
  }
  await persistActiveTasks(worktreePath)
}

export async function removeActiveTask(worktreePath: string, taskKey: string): Promise<void> {
  activeTasks = activeTasks.filter((t) => t.taskKey !== taskKey)
  await persistActiveTasks(worktreePath)
}

export async function loadActiveTask(
  worktreePath: string,
  options: { shouldApply?: () => boolean } = {},
): Promise<void> {
  const raw = await window.api.getPref(activeTaskKey(worktreePath))
  if (options.shouldApply && !options.shouldApply()) return
  if (raw) {
    try {
      const parsed = JSON.parse(raw) as ActiveTaskContext | ActiveTaskContext[]
      // Legacy prefs stored a single object; the store now keeps a list.
      activeTasks = Array.isArray(parsed) ? parsed : [parsed]
    } catch {
      activeTasks = []
    }
  } else {
    activeTasks = []
  }
}

export async function clearActiveTask(worktreePath: string): Promise<void> {
  activeTasks = []
  await persistActiveTasks(worktreePath)
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

/**
 * Patch the sidebar's copy of a panel task after the panel itself refreshed (e.g. a
 * status transition applied). Without this the left-sidebar chip kept the stale
 * status until the worktree was re-selected and the whole resolution re-ran.
 */
export function updatePanelTaskStatus(
  taskKey: string,
  status: string | undefined,
  statusCategory: string | undefined,
): void {
  const i = panelTasks.findIndex((t) => t.taskKey === taskKey)
  if (i < 0) return
  panelTasks[i] = { ...panelTasks[i], status, statusCategory }
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
  // Branch-derived task keys belong to the repository context. Personal/global trackers are
  // available for explicit links, but must never become the implicit owner merely by list order.
  const trackerId =
    resolvedConfig?.repoTrackerIds[0] ?? resolvedConfig?.config.trackers[0]?.id ?? ''

  const branchKeys = branch ? extractTaskKeys(branch) : []
  const keys = [
    ...activeTasks.map((t) => t.taskKey).filter((k) => !branchKeys.includes(k)),
    ...branchKeys,
  ]

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
      // Persisted linked-task contexts survive tracker failures; branch-derived keys fall
      // back to a bare-key context. Both are hydrated with the live status when reachable.
      const storedTask = activeTasks.find((t) => t.taskKey === key)
      const stored: PanelTaskContext | null = storedTask
        ? { ...storedTask, source: 'active' }
        : null
      try {
        // Address the tracker that owns the stored link; bare branch keys use the default.
        const ownerTrackerId = stored?.connectionId || trackerId || undefined
        const task = await window.api.trackerConfigFindTaskByKey(worktreePath, key, ownerTrackerId)
        // The tracker answered and doesn't know this key — a false match, drop it (unless it is
        // the explicitly linked task, which we keep as stored and flag as missing).
        if (!task) return stored ? { ...stored, missing: true } : null
        // Type icon proxied to a data: URL (authenticated tracker URL + CSP); cached in main.
        const typeIcon = task.typeIconUrl
          ? await window.api
              .taskTrackerImageAsDataUrl(worktreePath, task.typeIconUrl, ownerTrackerId)
              .catch(() => null)
          : null
        return {
          taskKey: task.key,
          summary: task.summary,
          connectionId: stored?.connectionId || trackerId,
          boardId: stored?.boardId,
          source: stored ? 'active' : 'branch',
          status: task.status,
          statusCategory: task.statusCategory,
          type: task.type,
          typeName: task.typeName,
          typeIcon: typeIcon ?? undefined,
        }
      } catch {
        // Offline / expired credentials: keep the bare key so the panel can still render it.
        return stored ?? { taskKey: key, summary: '', connectionId: trackerId, source: 'branch' }
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
