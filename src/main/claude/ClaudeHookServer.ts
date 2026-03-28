import http from 'http'
import { randomBytes, timingSafeEqual } from 'crypto'
import { is } from '@electron-toolkit/utils'
import type { HookEvent } from './types'

type HookEventHandler = (event: HookEvent) => Record<string, unknown> | void
type StatusUpdateHandler = (data: Record<string, unknown>) => void

const MAX_BODY_BYTES = 1_048_576 // 1 MB

export class ClaudeHookServer {
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
        if (is.dev) console.log(`[claude-status]`, JSON.stringify(data).slice(0, 300))
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
      const event: HookEvent = JSON.parse(body)
      const eventName = event.hook_event_name ?? 'unknown'
      const toolName = event.tool_name ? ` [${event.tool_name}]` : ''
      if (is.dev) console.log(`[claude-hook] ${eventName}${toolName}`)

      if (is.dev && event.tool_input) {
        console.log(`[claude-hook]   input:`, JSON.stringify(event.tool_input).slice(0, 200))
      }
      if (is.dev && event.tool_response !== undefined) {
        const respStr =
          typeof event.tool_response === 'string'
            ? event.tool_response
            : JSON.stringify(event.tool_response)
        console.log(`[claude-hook]   response:`, respStr.slice(0, 200))
      }

      response = this.onHookEvent(event)
    } catch (err) {
      if (is.dev) console.error(`[claude-hook] parse error:`, err)
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
