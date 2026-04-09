import type { DataChannelRpc } from '../../../../renderer-shared/rpc/DataChannelRpc'

/**
 * Bridges a PTY session's output to the remote peer by opening a secondary
 * WebSocket to the local {@link WsBridge} (the same one that feeds the desktop
 * `TerminalInstance`), then forwarding every chunk as an RPC event on the
 * `commands` DataChannelRpc. The peer subscribes to `pty.data.<sessionId>` and
 * writes each chunk into its xterm.js instance.
 *
 * Why a second WebSocket? The WsBridge already supports multi-client per
 * session and maintains a history buffer. Connecting another client is the
 * lowest-friction approach — no changes to `WsBridge.ts` or IPC needed.
 *
 * **Intentional vs unexpected close**: when the peer explicitly calls
 * `pty.unsubscribe` (e.g. the inline preview tearing down so the fullscreen
 * overlay can mount), we must NOT emit `pty.closed.<sessionId>` — the very
 * next `pty.subscribe` is about to open a fresh WS and any listener that
 * treats `pty.closed` as "session ended" would auto-exit fullscreen mode
 * mid-switch. We track intentional closes via a per-socket flag so only
 * unexpected WS drops (e.g. shell process exit, network failure) fire the
 * event.
 */

interface ForwardedSocket {
  ws: WebSocket
  intentional: boolean
}

export class PtyStreamForwarder {
  private sockets = new Map<string, ForwardedSocket>()

  constructor(private rpc: DataChannelRpc) {}

  subscribe(sessionId: string, wsUrl: string): void {
    if (this.sockets.has(sessionId)) return

    const entry: ForwardedSocket = {
      ws: new WebSocket(wsUrl),
      intentional: false,
    }
    this.sockets.set(sessionId, entry)

    entry.ws.onmessage = (ev) => {
      const text = typeof ev.data === 'string' ? ev.data : null
      if (text === null) return
      this.rpc.emit(`pty.data.${sessionId}`, text)
    }

    entry.ws.onerror = () => {
      console.warn(`[pty-forwarder] WS error for session ${sessionId}`)
    }

    entry.ws.onclose = () => {
      // If `unsubscribe` already removed our entry from the map and a new
      // `subscribe` has since replaced it with a fresh entry, leave the new
      // one alone — we're the stale ghost of a previous session.
      if (this.sockets.get(sessionId) === entry) {
        this.sockets.delete(sessionId)
      }
      // Only surface `pty.closed` to the peer when the close was *not*
      // caused by an explicit `unsubscribe`. Otherwise any listener that
      // treats the event as "session terminated" would fire spuriously
      // every time the peer swaps inline → fullscreen preview.
      if (!entry.intentional) {
        this.rpc.emit(`pty.closed.${sessionId}`, null)
      }
    }
  }

  unsubscribe(sessionId: string): void {
    const entry = this.sockets.get(sessionId)
    if (!entry) return
    // Mark and drop the entry synchronously so the *very next* `subscribe`
    // (which the peer may send in the same tick as `unsubscribe` when
    // switching between inline and fullscreen views) sees an empty slot
    // and opens a fresh WebSocket instead of reusing the dying one.
    entry.intentional = true
    this.sockets.delete(sessionId)
    try {
      entry.ws.close()
    } catch {
      /* ignore */
    }
  }

  dispose(): void {
    for (const [id, entry] of this.sockets) {
      entry.intentional = true
      try {
        entry.ws.close()
      } catch {
        /* ignore */
      }
      this.sockets.delete(id)
    }
  }
}
