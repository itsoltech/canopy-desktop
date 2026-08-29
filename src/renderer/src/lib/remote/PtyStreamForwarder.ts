import type { DataChannelRpc } from '../../../../renderer-shared/rpc/DataChannelRpc'

/**
 * Bridges a PTY session's output to the remote peer by subscribing to the
 * host renderer's preload IPC stream, then forwarding every chunk as an RPC
 * event on the `commands` DataChannelRpc. The peer subscribes to
 * `pty.data.<sessionId>` and writes each chunk into its xterm.js instance.
 *
 * **Intentional vs unexpected close**: when the peer explicitly calls
 * `pty.unsubscribe` (e.g. the inline preview tearing down so the fullscreen
 * overlay can mount), we must NOT emit `pty.closed.<sessionId>` — the very
 * next `pty.subscribe` is about to open a fresh IPC subscription and any
 * listener that treats `pty.closed` as "session ended" would auto-exit
 * fullscreen mode mid-switch. We track intentional closes per subscription
 * so only PTY stream closes from preload fire the event.
 */

interface ForwardedSubscription {
  cleanup: () => void
  intentional: boolean
}

interface TerminalStreamStateChange {
  state: 'paused' | 'resumed'
  pauseReasons: Array<'lock-screen' | 'suspend'>
}

export class PtyStreamForwarder {
  private subscriptions = new Map<string, ForwardedSubscription>()
  private offsets = new Map<string, number>()
  private desiredSessions = new Set<string>()
  private terminalStreamPaused = false
  private sawTerminalStreamEvent = false
  private cleanupTerminalStreamState: (() => void) | null = null

  constructor(private rpc: DataChannelRpc) {
    this.cleanupTerminalStreamState = window.api.onTerminalStreamStateChanged((data) => {
      this.sawTerminalStreamEvent = true
      this.applyTerminalStreamState(data)
    })

    void window.api
      .getTerminalStreamState()
      .then((data) => {
        if (!this.sawTerminalStreamEvent) this.applyTerminalStreamState(data)
      })
      .catch(() => undefined)
  }

  subscribe(sessionId: string): void {
    this.desiredSessions.add(sessionId)
    if (this.terminalStreamPaused) return
    if (this.subscriptions.has(sessionId)) return

    const entry: ForwardedSubscription = {
      cleanup: () => undefined,
      intentional: false,
    }
    this.subscriptions.set(sessionId, entry)

    entry.cleanup = window.api.subscribePtyData(
      sessionId,
      this.offsets.get(sessionId) ?? 0,
      (event) => {
        if (event.sessionId !== sessionId) return
        if (this.subscriptions.get(sessionId) !== entry) return

        const receivedChars = this.offsets.get(sessionId) ?? 0
        const eventEnd = event.offset + event.data.length
        if (eventEnd <= receivedChars) return

        const overlap = Math.max(0, receivedChars - event.offset)
        const chunk = overlap > 0 ? event.data.slice(overlap) : event.data
        const nextOffset = event.offset + overlap + chunk.length
        this.offsets.set(sessionId, nextOffset)
        if (!chunk) return

        this.rpc.emit(`pty.data.${sessionId}`, chunk)
      },
      (event) => {
        if (event.sessionId !== sessionId) return
        // If `unsubscribe` already removed our entry from the map and a new
        // `subscribe` has since replaced it with a fresh entry, leave the new
        // one alone. This callback belongs to a stale subscription.
        if (this.subscriptions.get(sessionId) === entry) {
          this.subscriptions.delete(sessionId)
        }
        // Only surface `pty.closed` to the peer when the close was *not*
        // caused by an explicit `unsubscribe`. Otherwise any listener that
        // treats the event as "session terminated" would fire spuriously
        // every time the peer swaps inline -> fullscreen preview.
        if (!entry.intentional) {
          this.desiredSessions.delete(sessionId)
          this.offsets.delete(sessionId)
          this.rpc.emit(`pty.closed.${sessionId}`, null)
        }
      },
      () => {
        if (this.subscriptions.get(sessionId) === entry) {
          this.subscriptions.delete(sessionId)
        }
        if (!entry.intentional) {
          this.desiredSessions.delete(sessionId)
          this.offsets.delete(sessionId)
          this.rpc.emit(`pty.closed.${sessionId}`, null)
        }
      },
    )
    if (this.subscriptions.get(sessionId) !== entry) {
      entry.intentional = true
      entry.cleanup()
      return
    }
  }

  unsubscribe(sessionId: string): void {
    this.desiredSessions.delete(sessionId)
    const entry = this.subscriptions.get(sessionId)
    if (!entry) {
      this.scheduleOffsetEviction(sessionId)
      return
    }
    // Mark and drop the entry synchronously so the *very next* `subscribe`
    // (which the peer may send in the same tick as `unsubscribe` when
    // switching between inline and fullscreen views) sees an empty slot
    // and opens a fresh IPC subscription instead of reusing the dying one.
    entry.intentional = true
    this.subscriptions.delete(sessionId)
    try {
      entry.cleanup()
    } catch {
      /* ignore */
    }
    this.scheduleOffsetEviction(sessionId)
  }

  /** Session ids the peer currently has an active PTY subscription for. */
  get activeSessionIds(): string[] {
    return [...this.subscriptions.keys()]
  }

  dispose(): void {
    this.cleanupTerminalStreamState?.()
    this.cleanupTerminalStreamState = null
    for (const [, entry] of this.subscriptions) {
      entry.intentional = true
      try {
        entry.cleanup()
      } catch {
        /* ignore */
      }
    }
    this.subscriptions.clear()
    this.desiredSessions.clear()
    this.offsets.clear()
  }

  private applyTerminalStreamState(data: TerminalStreamStateChange): void {
    this.terminalStreamPaused = data.state === 'paused' || data.pauseReasons.length > 0
    if (this.terminalStreamPaused) {
      this.pauseSubscriptions()
      return
    }
    for (const sessionId of [...this.desiredSessions]) {
      this.subscribe(sessionId)
    }
  }

  private pauseSubscriptions(): void {
    for (const [sessionId, entry] of [...this.subscriptions]) {
      entry.intentional = true
      this.subscriptions.delete(sessionId)
      try {
        entry.cleanup()
      } catch {
        /* ignore */
      }
    }
  }

  private scheduleOffsetEviction(sessionId: string): void {
    setTimeout(() => {
      if (this.desiredSessions.has(sessionId) || this.subscriptions.has(sessionId)) return
      this.offsets.delete(sessionId)
    }, 0)
  }
}
