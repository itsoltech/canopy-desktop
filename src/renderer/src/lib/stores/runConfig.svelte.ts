import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { workspaceState } from './workspace.svelte'
import { addToast } from './toast.svelte'
import { createLatestRequestGuard } from '../async/latestRequest'

// --- Types ---

interface RunConfiguration {
  name: string
  command: string
  args?: string
  cwd?: string
  max_instances?: number
  env?: Record<string, string>
  pre_run?: string
  post_run?: string
}

interface RunConfigSource {
  configDir: string
  relativePath: string
  file: { configurations: RunConfiguration[] }
}

interface RunningProcess {
  sessionId: string
  name: string
  configDir: string
  worktreePath: string
}

// --- State ---

let sources: RunConfigSource[] = $state([])
let selectedConfig: { configDir: string; name: string } | null = $state(null)
let isLoading = $state(false)
const runningProcesses = new SvelteMap<string, RunningProcess>()
// `configDir::name` of runs whose start (including `pre_run`, up to 30s) is still in flight.
const startingRuns = new SvelteSet<string>()
const discovery = createLatestRequestGuard()
let _cleanupBackgroundListener: (() => void) | null = null

// --- Derived ---

export function getSources(): RunConfigSource[] {
  return sources
}

export function getSelectedConfig(): { configDir: string; name: string } | null {
  return selectedConfig
}

export function getIsLoading(): boolean {
  return isLoading
}

export function getRunningProcesses(): Map<string, RunningProcess> {
  const current = workspaceState.selectedWorktreePath
  if (!current) return new SvelteMap()
  const filtered = new SvelteMap<string, RunningProcess>()
  for (const [id, proc] of runningProcesses) {
    if (proc.worktreePath === current) filtered.set(id, proc)
  }
  return filtered
}

export function getGroupedConfigs(): Map<
  string,
  { configDir: string; configurations: RunConfiguration[] }
> {
  const map = new SvelteMap<string, { configDir: string; configurations: RunConfiguration[] }>()
  for (const source of sources) {
    map.set(source.relativePath, {
      configDir: source.configDir,
      configurations: source.file.configurations,
    })
  }
  return map
}

// --- Actions ---

export async function discoverConfigs(): Promise<void> {
  const repoRoot = workspaceState.repoRoot
  if (!repoRoot) {
    // Also settles an in-flight discovery for the previous project, which now skips its finally.
    discovery.invalidate()
    sources = []
    isLoading = false
    return
  }
  // A slower discovery for the previous project must not replace the current project's list.
  const token = discovery.begin(repoRoot)
  isLoading = true
  try {
    const next = await window.api.runConfigDiscover(repoRoot)
    if (!discovery.isLatest(token)) return
    sources = next
    // A deleted or renamed selection would keep showing in the toolbar and fail on Play.
    if (
      selectedConfig &&
      !next.some(
        (source) =>
          source.configDir === selectedConfig?.configDir &&
          source.file.configurations.some((c) => c.name === selectedConfig?.name),
      )
    ) {
      selectedConfig = null
    }
  } catch (e) {
    if (!discovery.isLatest(token)) return
    console.warn('Failed to discover run configs:', e)
    sources = []
  } finally {
    if (discovery.isLatest(token)) isLoading = false
  }
}

export function selectRunConfig(configDir: string, name: string): void {
  selectedConfig = { configDir, name }
}

export function clearSelection(): void {
  selectedConfig = null
}

export async function addRunConfig(
  configDir: string,
  configuration: RunConfiguration,
): Promise<void> {
  await window.api.runConfigAddConfig(configDir, configuration)
  await discoverConfigs()
}

export async function updateRunConfig(
  configDir: string,
  oldName: string,
  configuration: RunConfiguration,
): Promise<void> {
  await window.api.runConfigUpdateConfig(configDir, oldName, configuration)
  await discoverConfigs()
}

export async function deleteRunConfig(configDir: string, name: string): Promise<void> {
  await window.api.runConfigDeleteConfig(configDir, name)
  await discoverConfigs()
}

function hydrateRunningProcesses(snapshots: RunningProcess[]): void {
  runningProcesses.clear()
  for (const snapshot of snapshots) {
    runningProcesses.set(snapshot.sessionId, snapshot)
  }
}

/**
 * Starts a run in the selected worktree. Callers open its tab in the returned `worktreePath`, not
 * the current selection: the user may switch worktrees while `pre_run` is still running.
 */
export async function executeRunConfig(
  configDir: string,
  name: string,
): Promise<{ sessionId: string; worktreePath: string } | null> {
  const key = `${configDir}::${name}`
  // A second click while the first start is in flight would launch (and pre_run) it twice.
  if (startingRuns.has(key)) return null
  const cwd = workspaceState.selectedWorktreePath
  if (!cwd) return null
  startingRuns.add(key)
  try {
    const result = await window.api.runConfigExecuteCommand(configDir, name, cwd)
    hydrateRunningProcesses(await window.api.runConfigListRunning())
    return { sessionId: result.sessionId, worktreePath: cwd }
  } catch (e) {
    addToast(`Failed to run "${name}": ${e instanceof Error ? e.message : String(e)}`)
    return null
  } finally {
    startingRuns.delete(key)
  }
}

export function initBackgroundListener(): void {
  if (_cleanupBackgroundListener) return
  const cleanupPty = window.api.onPtyExit((data) => {
    if (runningProcesses.has(data.sessionId)) {
      runningProcesses.delete(data.sessionId)
    }
  })
  const cleanupPostRun = window.api.onRunConfigPostRunResult((data) => {
    if (data.success) {
      addToast(`post_run "${data.command}" completed`)
    } else {
      addToast(`post_run "${data.command}" failed (exit ${data.exitCode})`)
    }
  })
  _cleanupBackgroundListener = () => {
    cleanupPty()
    cleanupPostRun()
  }
  void window.api
    .runConfigListRunning()
    .then((snapshots) => hydrateRunningProcesses(snapshots))
    .catch((e) => console.warn('Failed to list running run configs:', e))
}

export function cleanupBackgroundListener(): void {
  _cleanupBackgroundListener?.()
  _cleanupBackgroundListener = null
}
