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

export class PtyStreamForwarder {
  private subscriptions = new Map<string, ForwardedSubscription>()
  private offsets = new Map<string, number>()

  constructor(private rpc: DataChannelRpc) {}

  subscribe(sessionId: string): void {
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
          this.rpc.emit(`pty.closed.${sessionId}`, null)
        }
      },
      () => {
        if (this.subscriptions.get(sessionId) === entry) {
          this.subscriptions.delete(sessionId)
        }
        if (!entry.intentional) {
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
    const entry = this.subscriptions.get(sessionId)
    if (!entry) return
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
  }

  dispose(): void {
    for (const [, entry] of this.subscriptions) {
      entry.intentional = true
      try {
        entry.cleanup()
      } catch {
        /* ignore */
      }
    }
    this.subscriptions.clear()
  }
}
