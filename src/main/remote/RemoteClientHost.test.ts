import type { IncomingMessage, ServerResponse } from 'node:http'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import { RemoteClientHost } from './RemoteClientHost'

function fakeRequest(url: string): IncomingMessage {
  // handleRequest only reads the request target and method before it touches the bundle.
  return { url, method: 'GET' } as unknown as IncomingMessage
}

function fakeResponse(): { res: ServerResponse; status: () => number | undefined } {
  let statusCode: number | undefined
  const res = {
    writeHead: (code: number): void => {
      statusCode = code
    },
    end: (): void => {},
  }
  // Only the members used on the early-exit paths are needed.
  return { res: res as unknown as ServerResponse, status: (): number | undefined => statusCode }
}

describe('RemoteClientHost', () => {
  it('answers a request target the URL parser rejects with 400 instead of throwing', async () => {
    const host = new RemoteClientHost(path.join(os.tmpdir(), 'canopy-remote-bundle'))
    const { res, status } = fakeResponse()

    await expect(host.handleRequest(fakeRequest('//'), res)).resolves.toBe(true)
    expect(status()).toBe(400)
  })
})
