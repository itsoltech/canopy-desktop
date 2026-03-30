import { WebSocketServer } from 'ws'
import type { WebSocket as WsWebSocket } from 'ws'
import type { IPty } from 'node-pty'

const MAX_BUFFER_BYTES = 1_048_576 // 1 MB
const HEARTBEAT_INTERVAL = 30_000 // 30 s

interface Bridge {
  sessionId: string
  wss: WebSocketServer
  port: number
  clients: Set<WsWebSocket>
  cleanup: () => void
}

export class WsBridge {
  private bridges = new Map<string, Bridge>()

  async create(sessionId: string, ptyProcess: IPty): Promise<string> {
    const wss = new WebSocketServer({ port: 0, host: '127.0.0.1' })

    const port = await new Promise<number>((resolve) => {
      wss.on('listening', () => {
        const addr = wss.address()
        if (!addr || typeof addr === 'string') {
          resolve(0)
        } else {
          resolve(addr.port)
        }
      })
    })

    const clients = new Set<WsWebSocket>()

    // Rolling history buffer — always maintained so reconnecting clients
    // (e.g. after worktree switch) can restore the terminal content.
    const history: string[] = []
    let historyBytes = 0

    const onData = ptyProcess.onData((data) => {
      // Always append to rolling history
      history.push(data)
      historyBytes += data.length
      while (historyBytes > MAX_BUFFER_BYTES && history.length > 0) {
        historyBytes -= history[0].length
        history.shift()
      }

      // Forward to connected clients
      for (const client of clients) {
        if (client.readyState === 1) {
          client.send(data)
        }
      }
    })

    // Ping/pong heartbeat to detect dead connections
    const alive = new WeakSet<WsWebSocket>()

    const heartbeat = setInterval(() => {
      for (const client of clients) {
        if (!alive.has(client)) {
          client.terminate()
          return
        }
        alive.delete(client)
        client.ping()
      }
    }, HEARTBEAT_INTERVAL)

    wss.on('connection', (ws) => {
      clients.add(ws)
      alive.add(ws)

      ws.on('pong', () => alive.add(ws))

      // Send rolling history so reconnecting terminals restore their content
      for (const chunk of history) {
        ws.send(chunk)
      }

      ws.on('message', (data) => {
        ptyProcess.write(typeof data === 'string' ? data : data.toString())
      })

      ws.on('close', () => {
        clients.delete(ws)
      })

      ws.on('error', () => {
        clients.delete(ws)
      })
    })

    const cleanup = (): void => {
      clearInterval(heartbeat)
      onData.dispose()
    }

    this.bridges.set(sessionId, { sessionId, wss, port, clients, cleanup })
    return `ws://127.0.0.1:${port}`
  }

  /** Terminate all WS clients across all bridges (triggers renderer reconnection) */
  disconnectAllClients(): void {
    for (const bridge of this.bridges.values()) {
      for (const client of bridge.clients) {
        client.terminate()
      }
    }
  }

  destroy(sessionId: string): void {
    const bridge = this.bridges.get(sessionId)
    if (bridge) {
      bridge.cleanup()
      for (const client of bridge.clients) {
        client.close()
      }
      bridge.wss.close()
      this.bridges.delete(sessionId)
    }
  }

  disposeAll(): void {
    for (const [id] of this.bridges) {
      this.destroy(id)
    }
  }
}
