import { WebSocketServer } from 'ws'
import type { WebSocket as WsWebSocket } from 'ws'
import type { IPty } from 'node-pty'

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

    const onData = ptyProcess.onData((data) => {
      for (const client of clients) {
        if (client.readyState === 1) {
          client.send(data)
        }
      }
    })

    wss.on('connection', (ws) => {
      clients.add(ws)

      ws.on('message', (raw) => {
        try {
          const msg = JSON.parse(typeof raw === 'string' ? raw : raw.toString())
          if (msg.type === 'input') {
            ptyProcess.write(msg.data)
          } else if (msg.type === 'resize') {
            ptyProcess.resize(msg.cols, msg.rows)
          }
        } catch {
          // Not JSON — forward raw data
          ptyProcess.write(typeof raw === 'string' ? raw : raw.toString())
        }
      })

      ws.on('close', () => {
        clients.delete(ws)
      })

      ws.on('error', () => {
        clients.delete(ws)
      })
    })

    const cleanup = (): void => {
      onData.dispose()
    }

    this.bridges.set(sessionId, { sessionId, wss, port, clients, cleanup })
    return `ws://127.0.0.1:${port}`
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
