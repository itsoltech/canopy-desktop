import http from 'http'
import type { HookEvent } from './types'

type HookEventHandler = (event: HookEvent) => Record<string, unknown> | void
type StatusUpdateHandler = (data: Record<string, unknown>) => void

export class ClaudeHookServer {
  private server: http.Server
  private port = 0
  private onHookEvent: HookEventHandler
  private onStatusUpdate: StatusUpdateHandler

  constructor(onHookEvent: HookEventHandler, onStatusUpdate: StatusUpdateHandler) {
    this.onHookEvent = onHookEvent
    this.onStatusUpdate = onStatusUpdate
    this.server = http.createServer((req, res) => this.handleRequest(req, res))
  }

  async start(): Promise<number> {
    return new Promise((resolve) => {
      this.server.listen(0, '127.0.0.1', () => {
        const addr = this.server.address()
        this.port = typeof addr === 'object' && addr ? addr.port : 0
        resolve(this.port)
      })
    })
  }

  getPort(): number {
    return this.port
  }

  destroy(): void {
    this.server.close()
  }

  private async handleRequest(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    if (req.method !== 'POST') {
      res.writeHead(404)
      res.end()
      return
    }

    const body = await this.readBody(req)

    if (req.url === '/status') {
      try {
        this.onStatusUpdate(JSON.parse(body))
      } catch {
        // ignore parse errors
      }
      res.writeHead(200)
      res.end()
      return
    }

    if (req.url !== '/hook') {
      res.writeHead(404)
      res.end()
      return
    }

    let response: Record<string, unknown> | void = undefined
    try {
      const event: HookEvent = JSON.parse(body)
      response = this.onHookEvent(event)
    } catch {
      // ignore parse errors
    }

    res.writeHead(200, { 'Content-Type': 'application/json' })
    res.end(response ? JSON.stringify(response) : '{}')
  }

  private readBody(req: http.IncomingMessage): Promise<string> {
    return new Promise((resolve) => {
      let data = ''
      req.on('data', (chunk: string) => (data += chunk))
      req.on('end', () => resolve(data))
    })
  }
}
