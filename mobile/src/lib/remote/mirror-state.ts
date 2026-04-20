import type {
  HostInfoSnapshot,
  ProfileSnapshot,
  ProjectSnapshot,
  StateSnapshot,
  TabSnapshot,
  ToolSnapshot,
} from './protocol/state-snapshot'

/**
 * Peer-side mirror of the Canopy host's reactive stores. Mobile uses a
 * plain module-level object + listener set (compatible with React's
 * `useSyncExternalStore`) instead of Svelte 5 `$state` on desktop. The
 * store starts empty and is populated by {@link StateApplier} once the
 * RPC channel is up and `state.getSnapshot` has resolved.
 *
 * `state` is replaced with a new object reference on every mutation so
 * `useSyncExternalStore` can use reference equality to detect changes.
 */
export type MirrorState = {
  hydrated: boolean
  hostInfo: HostInfoSnapshot | null
  projects: ProjectSnapshot[]
  tabsByWorktree: Record<string, TabSnapshot[]>
  activeTabByWorktree: Record<string, string>
  activeWorktreePath: string | null
  tools: ToolSnapshot[]
  profiles: ProfileSnapshot[]
}

const emptyState: MirrorState = {
  hydrated: false,
  hostInfo: null,
  projects: [],
  tabsByWorktree: {},
  activeTabByWorktree: {},
  activeWorktreePath: null,
  tools: [],
  profiles: [],
}

let state: MirrorState = emptyState
const listeners = new Set<() => void>()

function notify(): void {
  for (const l of listeners) l()
}

export function getMirrorState(): MirrorState {
  return state
}

export function subscribeMirrorState(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function applyStateSnapshot(snapshot: StateSnapshot): void {
  state = {
    hydrated: true,
    hostInfo: snapshot.hostInfo,
    projects: snapshot.projects,
    tabsByWorktree: snapshot.tabsByWorktree,
    activeTabByWorktree: snapshot.activeTabByWorktree,
    activeWorktreePath: snapshot.activeWorktreePath,
    tools: snapshot.tools,
    profiles: snapshot.profiles ?? [],
  }
  notify()
}

export function applyProjectsDelta(projects: ProjectSnapshot[]): void {
  state = { ...state, projects }
  notify()
}

export function applyTabsDelta(tabsByWorktree: Record<string, TabSnapshot[]>): void {
  state = { ...state, tabsByWorktree }
  notify()
}

export function applyActiveTabDelta(activeTabByWorktree: Record<string, string>): void {
  state = { ...state, activeTabByWorktree }
  notify()
}

export function applyActiveWorktreeDelta(activeWorktreePath: string | null): void {
  state = { ...state, activeWorktreePath }
  notify()
}

export function applyToolsDelta(tools: ToolSnapshot[]): void {
  state = { ...state, tools }
  notify()
}

export function applyProfilesDelta(profiles: ProfileSnapshot[]): void {
  state = { ...state, profiles }
  notify()
}

export function resetMirrorState(): void {
  state = emptyState
  notify()
}
