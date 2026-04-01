import http from 'http'
import { randomBytes, timingSafeEqual } from 'crypto'
import { is } from '@electron-toolkit/utils'

type HookEventHandler = (event: Record<string, unknown>) => Record<string, unknown> | void
type StatusUpdateHandler = (data: Record<string, unknown>) => void

const MAX_BODY_BYTES = 1_048_576 // 1 MB

export class AgentHookServer {
  private server: http.Server
  private port = 0
  private authToken: string
  private onHookEvent: HookEventHandler
  private onStatusUpdate: StatusUpdateHandler

  constructor(onHookEvent: HookEventHandler, onStatusUpdate: StatusUpdateHandler) {
    this.onHookEvent = onHookEvent
    this.onStatusUpdate = onStatusUpdate
    this.authToken = randomBytes(32).toString('hex')
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

  getAuthToken(): string {
    return this.authToken
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

    // Validate auth token (timing-safe comparison)
    const provided = req.headers['x-canopy-auth']
    if (
      typeof provided !== 'string' ||
      provided.length !== this.authToken.length ||
      !timingSafeEqual(Buffer.from(provided), Buffer.from(this.authToken))
    ) {
      res.writeHead(403)
      res.end()
      return
    }

    const body = await this.readBody(req)
    if (!body) {
      res.writeHead(400)
      res.end()
      return
    }

    if (req.url === '/status') {
      try {
        const data = JSON.parse(body)
        if (is.dev) console.log(`[agent-status]`, JSON.stringify(data).slice(0, 300))
        this.onStatusUpdate(data)
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
      const event: Record<string, unknown> = JSON.parse(body)
      const eventName = (event.hook_event_name as string) ?? 'unknown'
      const toolName = event.tool_name ? ` [${event.tool_name}]` : ''
      if (is.dev) console.log(`[agent-hook] ${eventName}${toolName}`)

      if (is.dev) {
        const skip = new Set(['hook_event_name', 'tool_name'])
        for (const [k, v] of Object.entries(event)) {
          if (skip.has(k) || v === undefined || v === null) continue
          const str = typeof v === 'string' ? v : JSON.stringify(v)
          console.log(`[agent-hook]   ${k}: ${str}`)
        }
      }

      response = this.onHookEvent(event)
    } catch (err) {
      if (is.dev) console.error(`[agent-hook] parse error:`, err)
    }

    res.writeHead(200, { 'Content-Type': 'application/json' })
    res.end(response ? JSON.stringify(response) : '{}')
  }

  private readBody(req: http.IncomingMessage): Promise<string> {
    return new Promise((resolve) => {
      let data = ''
      let bytes = 0
      req.on('data', (chunk: string) => {
        bytes += Buffer.byteLength(chunk)
        if (bytes > MAX_BODY_BYTES) {
          req.destroy()
          resolve('')
          return
        }
        data += chunk
      })
      req.on('end', () => resolve(data))
      req.on('error', () => resolve(''))
    })
  }
}
