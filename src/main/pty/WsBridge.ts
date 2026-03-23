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

    // Buffer PTY output when no client is connected.
    // Re-activates after all clients disconnect (e.g. sleep/wake).
    let buffer: string[] = []
    let bufferBytes = 0
    let connected = false

    const onData = ptyProcess.onData((data) => {
      if (!connected) {
        buffer.push(data)
        bufferBytes += data.length
        while (bufferBytes > MAX_BUFFER_BYTES && buffer.length > 0) {
          bufferBytes -= buffer[0].length
          buffer.shift()
        }
        return
      }
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

      // Flush buffered data to the reconnecting client
      if (!connected) {
        connected = true
        for (const chunk of buffer) {
          ws.send(chunk)
        }
        buffer = []
        bufferBytes = 0
      }

      ws.on('message', (data) => {
        ptyProcess.write(typeof data === 'string' ? data : data.toString())
      })

      ws.on('close', () => {
        clients.delete(ws)
        if (clients.size === 0) {
          connected = false
        }
      })

      ws.on('error', () => {
        clients.delete(ws)
        if (clients.size === 0) {
          connected = false
        }
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
