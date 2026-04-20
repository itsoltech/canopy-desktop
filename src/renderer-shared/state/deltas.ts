import type { ProfileSnapshot, ProjectSnapshot, TabSnapshot, ToolSnapshot } from './snapshot'

/**
 * Delta event topics the Canopy host emits through the RPC `state` event
 * stream. Each delta carries a *full replacement* for its topic rather than
 * fine-grained add/remove/update patches — payloads are small (a dozen
 * entries at most) and full replace is dramatically simpler to apply on the
 * peer side. JSON-patch style diffs are deferred to the verification phase.
 *
 * Topic names are kept symmetric with {@link StateSnapshot} fields so the
 * peer's state applier can map `topic` → setter by convention.
 */
export type StateDelta =
  | { topic: 'projects'; data: ProjectSnapshot[] }
  | { topic: 'tabs'; data: Record<string, TabSnapshot[]> }
  | { topic: 'activeTab'; data: Record<string, string> }
  | { topic: 'activeWorktree'; data: string | null }
  | { topic: 'tools'; data: ToolSnapshot[] }
  | { topic: 'profiles'; data: ProfileSnapshot[] }

/** All topic names known to the delta protocol. Useful as a runtime guard. */
export const STATE_DELTA_TOPICS = [
  'projects',
  'tabs',
  'activeTab',
  'activeWorktree',
  'tools',
  'profiles',
] as const

export type StateDeltaTopic = (typeof STATE_DELTA_TOPICS)[number]
