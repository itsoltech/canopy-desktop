import http from 'http'
import { randomBytes, timingSafeEqual } from 'crypto'
import type { SkillInstaller } from './SkillInstaller'
import type { SkillRegistry } from './SkillRegistry'
import type { SkillInstallOptions } from './types'

export class SkillsCliServer {
  private server: http.Server | null = null
  private port = 0
  readonly authToken = randomBytes(32).toString('hex')

  constructor(
    private registry: SkillRegistry,
    private installer: SkillInstaller,
  ) {}

  async start(): Promise<number> {
    if (this.server) return this.port

    return new Promise((resolve, reject) => {
      const server = http.createServer((req, res) => this.handleRequest(req, res))
      server.listen(0, '127.0.0.1', () => {
        const addr = server.address()
        if (addr && typeof addr === 'object') {
          this.port = addr.port
          this.server = server
          resolve(this.port)
        } else {
          reject(new Error('Failed to get server address'))
        }
      })
      server.on('error', reject)
    })
  }

  stop(): void {
    this.server?.close()
    this.server = null
  }

  private async handleRequest(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    if (req.method !== 'POST') {
      res.writeHead(404)
      res.end()
      return
    }

    // Validate auth token
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

    // Read body
    const chunks: Buffer[] = []
    for await (const chunk of req) {
      chunks.push(typeof chunk === 'string' ? Buffer.from(chunk) : chunk)
    }
    const body = Buffer.concat(chunks).toString('utf-8')

    let payload: { action: string; args?: Record<string, unknown> }
    try {
      payload = JSON.parse(body)
    } catch {
      res.writeHead(400, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'Invalid JSON' }))
      return
    }

    try {
      const result = await this.dispatch(payload.action, payload.args ?? {})
      res.writeHead(200, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify(result))
    } catch (e) {
      res.writeHead(500, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: e instanceof Error ? e.message : String(e) }))
    }
  }

  private async dispatch(action: string, args: Record<string, unknown>): Promise<unknown> {
    switch (action) {
      case 'list': {
        return { skills: this.registry.list(args as never) }
      }
      case 'get': {
        const skill = this.registry.get(args.id as string)
        return skill ?? { error: `Skill not found: ${args.id}` }
      }
      case 'install': {
        const result = await this.installer.install(args as unknown as SkillInstallOptions)
        if (result.isErr()) throw new Error(result.error._tag + ': ' + JSON.stringify(result.error))
        this.registry.refresh()
        return result.value
      }
      case 'remove': {
        const result = this.installer.remove(
          args.id as string,
          args.workspacePath as string | undefined,
        )
        if (result.isErr()) throw new Error(result.error._tag + ': ' + JSON.stringify(result.error))
        this.registry.refresh()
        return { success: true }
      }
      case 'update': {
        const result = await this.installer.update(
          args.id as string,
          args.workspacePath as string | undefined,
        )
        if (result.isErr()) throw new Error(result.error._tag + ': ' + JSON.stringify(result.error))
        this.registry.refresh()
        return result.value
      }
      default:
        throw new Error(`Unknown action: ${action}`)
    }
  }
}
