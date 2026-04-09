import type {
  HostInfoSnapshot,
  ProjectSnapshot,
  StateSnapshot,
  TabSnapshot,
  ToolSnapshot,
} from '../../../../../renderer-shared/state/snapshot'

/**
 * Peer-side mirror of the Canopy host's reactive stores. Shape matches
 * {@link StateSnapshot} 1:1 so {@link StateApplier} can patch individual
 * fields when a delta arrives without reshuffling the graph.
 *
 * The store starts empty and is populated by the applier once the RPC
 * channel is up and `state.getSnapshot` has resolved. Components consume
 * it via Svelte 5 `$state` reactivity — touching any of these fields in a
 * `$derived` or `$effect` subscribes automatically.
 */
export const mirrorState: {
  /** True after the initial snapshot has been applied. */
  hydrated: boolean
  hostInfo: HostInfoSnapshot | null
  projects: ProjectSnapshot[]
  tabsByWorktree: Record<string, TabSnapshot[]>
  activeTabByWorktree: Record<string, string>
  activeWorktreePath: string | null
  tools: ToolSnapshot[]
} = $state({
  hydrated: false,
  hostInfo: null,
  projects: [],
  tabsByWorktree: {},
  activeTabByWorktree: {},
  activeWorktreePath: null,
  tools: [],
})

/**
 * Replace the entire mirror with a fresh snapshot. Used on initial hydration
 * and on reconnect to resync after a gap.
 */
export function applyStateSnapshot(snapshot: StateSnapshot): void {
  mirrorState.hostInfo = snapshot.hostInfo
  mirrorState.projects = snapshot.projects
  mirrorState.tabsByWorktree = snapshot.tabsByWorktree
  mirrorState.activeTabByWorktree = snapshot.activeTabByWorktree
  mirrorState.activeWorktreePath = snapshot.activeWorktreePath
  mirrorState.tools = snapshot.tools
  mirrorState.hydrated = true
}

/** Reset the mirror when the session disconnects. */
export function resetMirrorState(): void {
  mirrorState.hydrated = false
  mirrorState.hostInfo = null
  mirrorState.projects = []
  mirrorState.tabsByWorktree = {}
  mirrorState.activeTabByWorktree = {}
  mirrorState.activeWorktreePath = null
  mirrorState.tools = []
}
