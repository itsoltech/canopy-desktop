import http from 'http'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AgentHookRouter } from './AgentHookServer'

vi.mock('@electron-toolkit/utils', () => ({ is: { dev: false } }))

const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms))

/** POSTs `body` in two writes, split at byte `splitAt`, so the server sees separate chunks. */
async function postInTwoChunks(
  port: number,
  path: string,
  authToken: string,
  body: Buffer,
  splitAt: number,
): Promise<number> {
  return new Promise((resolve, reject) => {
    const req = http.request(
      {
        host: '127.0.0.1',
        port,
        path,
        method: 'POST',
        headers: { 'x-canopy-auth': authToken },
        // No keep-alive socket left behind for dispose() to wait on.
        agent: false,
      },
      (res) => {
        res.resume()
        res.on('end', () => resolve(res.statusCode ?? 0))
      },
    )
    req.on('error', reject)
    req.write(body.subarray(0, splitAt))
    void delay(50).then(() => req.end(body.subarray(splitAt)))
  })
}

describe('AgentHookRouter', () => {
  const router = new AgentHookRouter()

  afterEach(() => router.dispose())

  it('decodes a UTF-8 character split across request chunks intact', async () => {
    const received: Record<string, unknown>[] = []
    const { port, path, authToken } = await router.addSession(
      'session-utf8',
      (event) => {
        received.push(event)
      },
      () => {},
    )
    const message = 'Zażółć gęślą jaźń'
    const body = Buffer.from(JSON.stringify({ hook_event_name: 'Notification', message }))
    // Split inside the two-byte "ż" so each half arrives as its own chunk.
    const splitAt = body.indexOf(Buffer.from('ż')) + 1

    const status = await postInTwoChunks(port, `${path}/hook`, authToken, body, splitAt)

    expect(status).toBe(200)
    expect(received).toEqual([{ hook_event_name: 'Notification', message }])
  })
})
