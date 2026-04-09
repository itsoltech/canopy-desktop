import { SvelteMap } from 'svelte/reactivity'
import { workspaceState } from './workspace.svelte'
import { addToast } from './toast.svelte'

// --- Types ---

interface RunConfiguration {
  name: string
  command: string
  args?: string
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
    sources = []
    return
  }
  isLoading = true
  try {
    sources = await window.api.runConfigDiscover(repoRoot)
  } catch (e) {
    console.warn('Failed to discover run configs:', e)
    sources = []
  } finally {
    isLoading = false
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

function getGlobalRunningCount(name: string): number {
  let count = 0
  for (const proc of runningProcesses.values()) {
    if (proc.name === name) count++
  }
  return count
}

function findConfigEntry(configDir: string, name: string): RunConfiguration | undefined {
  for (const source of sources) {
    if (source.configDir !== configDir) continue
    return source.file.configurations.find((c) => c.name === name)
  }
  return undefined
}

export async function executeRunConfig(
  configDir: string,
  name: string,
): Promise<{ sessionId: string; wsUrl: string } | null> {
  try {
    const config = findConfigEntry(configDir, name)
    const maxInstances = config?.max_instances ?? 0
    if (maxInstances > 0) {
      const current = getGlobalRunningCount(name)
      if (current >= maxInstances) {
        addToast(`"${name}" is already running (max ${maxInstances})`)
        return null
      }
    }
    const cwd = workspaceState.selectedWorktreePath
    if (!cwd) return null
    const result = await window.api.runConfigExecute(configDir, name, cwd)
    runningProcesses.set(result.sessionId, {
      sessionId: result.sessionId,
      name,
      configDir,
      worktreePath: cwd,
    })
    return result
  } catch (e) {
    addToast(`Failed to run "${name}": ${e instanceof Error ? e.message : String(e)}`)
    return null
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
}

export function cleanupBackgroundListener(): void {
  _cleanupBackgroundListener?.()
  _cleanupBackgroundListener = null
}
