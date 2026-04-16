// Keep in sync with src/renderer-shared/state/deltas.ts when the wire protocol changes.

import type { ProfileSnapshot, ProjectSnapshot, TabSnapshot, ToolSnapshot } from './state-snapshot'

/**
 * Delta event topics the Canopy host emits through the RPC `state` event
 * stream. Each delta carries a *full replacement* for its topic rather than
 * fine-grained add/remove/update patches.
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
