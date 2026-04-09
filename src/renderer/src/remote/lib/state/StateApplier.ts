import { match } from 'ts-pattern'
import type {
  ProjectSnapshot,
  TabSnapshot,
  ToolSnapshot,
} from '../../../../../renderer-shared/state/snapshot'
import { STATE_DELTA_TOPICS } from '../../../../../renderer-shared/state/deltas'
import type { RemoteApi } from '../api'
import { applyStateSnapshot, mirrorState, resetMirrorState } from './mirrorState.svelte'

/**
 * Peer-side adapter that pulls the initial snapshot from the host via
 * `state.getSnapshot` and then subscribes to per-topic delta events so the
 * mirror stays live.
 *
 * The applier is intentionally passive: it never initiates queries beyond
 * the initial hydration. Every delta event is a full-replace for its topic,
 * so applying it is just `mirrorState.<field> = msg.data`.
 */
export class StateApplier {
  private unsubscribers: Array<() => void> = []
  private disposed = false
  private api: RemoteApi | null = null

  async start(api: RemoteApi): Promise<void> {
    if (this.disposed) return
    this.api = api

    // Subscribe BEFORE fetching the snapshot so any changes that happen
    // between snapshot-build and snapshot-apply are caught. (Race window is
    // small but non-zero when the host is reactively re-emitting effects
    // during initial hydration.)
    for (const topic of STATE_DELTA_TOPICS) {
      const unsub = api.subscribe(topic, (data) => this.applyTopic(topic, data))
      this.unsubscribers.push(unsub)
    }

    try {
      const snapshot = await api.state.getSnapshot()
      if (this.disposed) return
      applyStateSnapshot(snapshot)
    } catch (err) {
      console.warn('[state-applier] initial snapshot failed:', err)
    }
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    for (const unsub of this.unsubscribers) {
      try {
        unsub()
      } catch {
        /* ignore */
      }
    }
    this.unsubscribers = []
    this.api = null
    resetMirrorState()
  }

  /**
   * Apply a single delta event to the mirror store. `data` is untyped here
   * because `DataChannelRpc.subscribe` is generic — we narrow it per topic
   * with `ts-pattern` so future topics stay exhaustive at compile time.
   */
  private applyTopic(topic: (typeof STATE_DELTA_TOPICS)[number], data: unknown): void {
    match(topic)
      .with('projects', () => {
        if (!Array.isArray(data)) return
        mirrorState.projects = data as ProjectSnapshot[]
      })
      .with('tabs', () => {
        if (typeof data !== 'object' || data === null) return
        mirrorState.tabsByWorktree = data as Record<string, TabSnapshot[]>
      })
      .with('activeTab', () => {
        if (typeof data !== 'object' || data === null) return
        mirrorState.activeTabByWorktree = data as Record<string, string>
      })
      .with('activeWorktree', () => {
        if (data !== null && typeof data !== 'string') return
        mirrorState.activeWorktreePath = data as string | null
      })
      .with('tools', () => {
        if (!Array.isArray(data)) return
        mirrorState.tools = data as ToolSnapshot[]
      })
      .exhaustive()
  }
}
