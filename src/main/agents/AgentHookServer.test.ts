import http from 'node:http'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AgentHookRouter } from './AgentHookServer'

vi.mock('@electron-toolkit/utils', () => ({ is: { dev: false } }))

function post(port: number, path: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const req = http.request(
      { host: '127.0.0.1', port, path, method: 'POST', agent: false },
      (res) => {
        res.resume()
        resolve(res.statusCode ?? 0)
      },
    )
    req.on('error', reject)
    req.end('{}')
  })
}

describe('AgentHookRouter', () => {
  const router = new AgentHookRouter()

  afterEach(() => {
    router.dispose()
  })

  it('answers a malformed percent-encoded session path with 404', async () => {
    const { port } = await router.addSession(
      'session-1',
      () => {},
      () => {},
    )

    await expect(post(port, '/session/%E0%A4%A/hook')).resolves.toBe(404)
  })
})
