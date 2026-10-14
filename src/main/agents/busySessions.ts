import type { AgentAdapter, NormalizedEventName } from './types'

/**
 * Agent panes that are mid-turn, keyed by PTY session id. The tab and window close
 * confirmations read it.
 *
 * A compaction normally runs inside a turn, whose Stop settles the pane. One that starts while
 * the pane is idle has no turn around it: Claude Code 2.1.299 compacts a parked conversation of
 * 200k+ tokens shortly before its one-hour prompt cache expires, and no Stop follows. Such a
 * compaction is settled by its own end instead.
 */
export class BusySessions {
  private busy = new Set<string>()
  private idleCompactions = new Set<string>()

  /**
   * Applies one hook event. Returns true when the event starts or ends a compaction that began
   * while the pane was idle.
   */
  track(
    ptySessionId: string,
    adapter: Pick<AgentAdapter, 'busyEvents' | 'idleEvents'>,
    rawName: string,
    event: NormalizedEventName,
  ): boolean {
    let startsIdleCompaction = false
    if (event === 'BeforeCompact') {
      startsIdleCompaction = !this.busy.has(ptySessionId)
      if (startsIdleCompaction) this.idleCompactions.add(ptySessionId)
      else this.idleCompactions.delete(ptySessionId)
    } else if (event === 'AfterCompact' && this.idleCompactions.delete(ptySessionId)) {
      this.busy.delete(ptySessionId)
      return true
    } else if (adapter.busyEvents.has(rawName)) {
      // A turn has started, and its Stop will settle the pane.
      this.idleCompactions.delete(ptySessionId)
    }

    if (adapter.busyEvents.has(rawName)) {
      this.busy.add(ptySessionId)
    } else if (adapter.idleEvents.has(rawName)) {
      this.busy.delete(ptySessionId)
    }
    return startsIdleCompaction
  }

  has(ptySessionId: string): boolean {
    return this.busy.has(ptySessionId)
  }

  rekey(from: string, to: string): void {
    if (this.busy.delete(from)) this.busy.add(to)
    if (this.idleCompactions.delete(from)) this.idleCompactions.add(to)
  }

  delete(ptySessionId: string): void {
    this.busy.delete(ptySessionId)
    this.idleCompactions.delete(ptySessionId)
  }

  clear(): void {
    this.busy.clear()
    this.idleCompactions.clear()
  }
}
