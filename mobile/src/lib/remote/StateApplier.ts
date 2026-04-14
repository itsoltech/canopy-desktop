// Ported from src/renderer/src/remote/lib/state/StateApplier.ts

import { match } from 'ts-pattern'

import type { RemoteApi } from './RemoteApi'
import {
  applyActiveTabDelta,
  applyActiveWorktreeDelta,
  applyProjectsDelta,
  applyStateSnapshot,
  applyTabsDelta,
  applyToolsDelta,
  resetMirrorState,
} from './mirror-state'
import { STATE_DELTA_TOPICS } from './protocol/state-deltas'
import type { ProjectSnapshot, TabSnapshot, ToolSnapshot } from './protocol/state-snapshot'

/**
 * Peer-side adapter that pulls the initial snapshot from the host via
 * `state.getSnapshot` and then subscribes to per-topic delta events so the
 * mirror stays live. Every delta is a full-replace for its topic.
 */
export class StateApplier {
  private unsubscribers: Array<() => void> = []
  private disposed = false
  private api: RemoteApi | null = null

  async start(api: RemoteApi): Promise<void> {
    if (this.disposed) return
    this.api = api

    // Subscribe BEFORE fetching the snapshot so any changes that happen
    // between snapshot-build and snapshot-apply are caught.
    for (const topic of STATE_DELTA_TOPICS) {
      const unsub = api.subscribe(topic, (data: unknown) => this.applyTopic(topic, data))
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

  private applyTopic(topic: (typeof STATE_DELTA_TOPICS)[number], data: unknown): void {
    match(topic)
      .with('projects', () => {
        if (!Array.isArray(data)) return
        applyProjectsDelta(data as ProjectSnapshot[])
      })
      .with('tabs', () => {
        if (typeof data !== 'object' || data === null) return
        applyTabsDelta(data as Record<string, TabSnapshot[]>)
      })
      .with('activeTab', () => {
        if (typeof data !== 'object' || data === null) return
        applyActiveTabDelta(data as Record<string, string>)
      })
      .with('activeWorktree', () => {
        if (data !== null && typeof data !== 'string') return
        applyActiveWorktreeDelta(data as string | null)
      })
      .with('tools', () => {
        if (!Array.isArray(data)) return
        applyToolsDelta(data as ToolSnapshot[])
      })
      .exhaustive()
  }
}
