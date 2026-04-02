import { WebSocketServer } from 'ws'
import type { WebSocket as WsWebSocket } from 'ws'
import type { IPty } from 'node-pty'

const MAX_BUFFER_BYTES = 1_048_576 // 1 MB
const HEARTBEAT_INTERVAL = 30_000 // 30 s

interface HistoryChunk {
  start: number
  end: number
  data: string
}

interface Bridge {
  sessionId: string
  clients: Set<WsWebSocket>
  history: HistoryChunk[]
  historyBytes: number
  totalChars: number
  write: (data: string) => void
  cleanup: () => void
}

export class WsBridge {
  private bridges = new Map<string, Bridge>()
  private wss: WebSocketServer | null = null
  private serverReady: Promise<number> | null = null
  private port = 0
  private heartbeat: ReturnType<typeof setInterval> | null = null
  private alive = new WeakSet<WsWebSocket>()

  async create(sessionId: string, ptyProcess: IPty): Promise<string> {
    const port = await this.ensureServer()
    const clients = new Set<WsWebSocket>()

    const bridge: Bridge = {
      sessionId,
      clients,
      history: [],
      historyBytes: 0,
      totalChars: 0,
      write: (data: string) => ptyProcess.write(data),
      cleanup: () => {},
    }

    const onData = ptyProcess.onData((data) => {
      const start = bridge.totalChars
      bridge.totalChars += data.length
      bridge.history.push({ start, end: bridge.totalChars, data })
      bridge.historyBytes += data.length

      while (bridge.historyBytes > MAX_BUFFER_BYTES && bridge.history.length > 0) {
        const removed = bridge.history.shift()
        if (!removed) break
        bridge.historyBytes -= removed.data.length
      }

      for (const client of clients) {
        if (client.readyState === 1) {
          client.send(data)
        }
      }
    })

    bridge.cleanup = () => {
      onData.dispose()
    }

    this.bridges.set(sessionId, bridge)
    return `ws://127.0.0.1:${port}/${encodeURIComponent(sessionId)}`
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
    if (!bridge) return

    bridge.cleanup()
    for (const client of bridge.clients) {
      client.close()
    }
    this.bridges.delete(sessionId)
    this.closeServerIfIdle()
  }

  disposeAll(): void {
    for (const [id] of this.bridges) {
      this.destroy(id)
    }
  }

  private async ensureServer(): Promise<number> {
    if (this.serverReady) return this.serverReady

    const wss = new WebSocketServer({ port: 0, host: '127.0.0.1' })
    this.wss = wss

    wss.on('connection', (ws, req) => {
      const sessionId = this.parseSessionId(req.url)
      if (!sessionId) {
        ws.close()
        return
      }

      const bridge = this.bridges.get(sessionId)
      if (!bridge) {
        ws.close()
        return
      }

      bridge.clients.add(ws)
      this.alive.add(ws)

      ws.on('pong', () => this.alive.add(ws))

      const offset = this.parseOffset(req.url)
      this.sendHistory(ws, bridge, offset)

      ws.on('message', (data) => {
        bridge.write(typeof data === 'string' ? data : data.toString())
      })

      const removeClient = (): void => {
        bridge.clients.delete(ws)
      }

      ws.on('close', removeClient)
      ws.on('error', removeClient)
    })

    this.serverReady = new Promise<number>((resolve, reject) => {
      let startupPending = true

      const handleServerError = (error: Error): void => {
        if (!startupPending) {
          console.error('WebSocket bridge server error:', error)
          return
        }

        startupPending = false
        wss.off('listening', handleListening)
        this.closeServerIfIdle()
        reject(error)
      }

      const handleListening = (): void => {
        startupPending = false
        const addr = wss.address()
        if (!addr || typeof addr === 'string') {
          this.port = 0
        } else {
          this.port = addr.port
        }
        this.startHeartbeat()
        resolve(this.port)
      }

      wss.on('error', handleServerError)
      wss.once('listening', handleListening)
    })

    return this.serverReady
  }

  private startHeartbeat(): void {
    if (this.heartbeat) return

    this.heartbeat = setInterval(() => {
      for (const bridge of this.bridges.values()) {
        for (const client of bridge.clients) {
          if (!this.alive.has(client)) {
            client.terminate()
            continue
          }
          this.alive.delete(client)
          client.ping()
        }
      }
    }, HEARTBEAT_INTERVAL)
  }

  private closeServerIfIdle(): void {
    if (this.bridges.size > 0) return

    if (this.heartbeat) {
      clearInterval(this.heartbeat)
      this.heartbeat = null
    }

    if (this.wss) {
      this.wss.close()
      this.wss = null
    }

    this.serverReady = null
    this.port = 0
    this.alive = new WeakSet<WsWebSocket>()
  }

  private parseSessionId(rawUrl?: string): string | null {
    try {
      const url = new URL(rawUrl ?? '/', 'ws://127.0.0.1')
      const sessionId = decodeURIComponent(url.pathname.slice(1))
      return sessionId || null
    } catch {
      return null
    }
  }

  private parseOffset(rawUrl?: string): number {
    try {
      const url = new URL(rawUrl ?? '/', 'ws://127.0.0.1')
      const offset = Number(url.searchParams.get('offset') ?? '0')
      return Number.isFinite(offset) && offset >= 0 ? offset : 0
    } catch {
      return 0
    }
  }

  private sendHistory(ws: WsWebSocket, bridge: Bridge, offset: number): void {
    for (const chunk of bridge.history) {
      if (chunk.end <= offset) continue
      if (offset > chunk.start) {
        ws.send(chunk.data.slice(offset - chunk.start))
      } else {
        ws.send(chunk.data)
      }
    }
  }
}
