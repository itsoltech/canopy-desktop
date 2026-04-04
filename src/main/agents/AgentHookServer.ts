import http from 'http'
import { randomBytes, timingSafeEqual } from 'crypto'
import { is } from '@electron-toolkit/utils'

type HookEventHandler = (event: Record<string, unknown>) => Record<string, unknown> | void
type StatusUpdateHandler = (data: Record<string, unknown>) => void

const MAX_BODY_BYTES = 1_048_576 // 1 MB

interface SessionEntry {
  authToken: string
  onHookEvent: HookEventHandler
  onStatusUpdate: StatusUpdateHandler
}

/**
 * Shared HTTP server that routes agent hook/status requests to the correct
 * session by URL path. One server handles all agent sessions instead of
 * spawning a separate HTTP server per session.
 *
 * URL pattern: POST /session/<sessionId>/hook
 *              POST /session/<sessionId>/status
 */
export class AgentHookRouter {
  private server: http.Server | null = null
  private port = 0
  private serverReady: Promise<number> | null = null
  private sessions = new Map<string, SessionEntry>()

  async addSession(
    sessionId: string,
    onHookEvent: HookEventHandler,
    onStatusUpdate: StatusUpdateHandler,
  ): Promise<{ port: number; path: string; authToken: string }> {
    const authToken = randomBytes(32).toString('hex')
    this.sessions.set(sessionId, { authToken, onHookEvent, onStatusUpdate })

    const port = await this.ensureServer()
    const path = `/session/${encodeURIComponent(sessionId)}`
    return { port, path, authToken }
  }

  removeSession(sessionId: string): void {
    this.sessions.delete(sessionId)
    this.closeServerIfIdle()
  }

  dispose(): void {
    this.sessions.clear()
    this.closeServerIfIdle()
  }

  private async ensureServer(): Promise<number> {
    if (this.serverReady) return this.serverReady

    const server = http.createServer((req, res) => this.handleRequest(req, res))
    this.server = server

    this.serverReady = new Promise<number>((resolve, reject) => {
      server.on('error', (err) => {
        this.closeServerIfIdle()
        reject(err)
      })
      server.listen(0, '127.0.0.1', () => {
        const addr = server.address()
        this.port = typeof addr === 'object' && addr ? addr.port : 0
        resolve(this.port)
      })
    })

    return this.serverReady
  }

  private closeServerIfIdle(): void {
    if (this.sessions.size > 0) return

    if (this.server) {
      this.server.close()
      this.server = null
    }
    this.serverReady = null
    this.port = 0
  }

  private async handleRequest(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    if (req.method !== 'POST') {
      res.writeHead(404)
      res.end()
      return
    }

    // Parse URL: /session/<sessionId>/hook or /session/<sessionId>/status
    const match = req.url?.match(/^\/session\/([^/]+)\/(hook|status)$/)
    if (!match) {
      res.writeHead(404)
      res.end()
      return
    }

    const sessionId = decodeURIComponent(match[1])
    const endpoint = match[2]
    const session = this.sessions.get(sessionId)

    if (!session) {
      res.writeHead(404)
      res.end()
      return
    }

    // Validate per-session auth token
    const provided = req.headers['x-canopy-auth']
    if (
      typeof provided !== 'string' ||
      provided.length !== session.authToken.length ||
      !timingSafeEqual(Buffer.from(provided), Buffer.from(session.authToken))
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

    if (endpoint === 'status') {
      try {
        const data = JSON.parse(body)
        if (is.dev) console.log(`[agent-status]`, JSON.stringify(data).slice(0, 300))
        session.onStatusUpdate(data)
      } catch {
        // ignore parse errors
      }
      res.writeHead(200)
      res.end()
      return
    }

    // endpoint === 'hook'
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

      response = session.onHookEvent(event)
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
