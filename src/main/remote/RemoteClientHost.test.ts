import type { IncomingMessage, ServerResponse } from 'node:http'
import { describe, expect, it, vi } from 'vitest'
import { RemoteClientHost } from './RemoteClientHost'

describe('RemoteClientHost.handleRequest', () => {
  it('answers an unparseable request target with 400 instead of rejecting', async () => {
    const req = { url: '//', method: 'GET' }
    const res = { writeHead: vi.fn(), end: vi.fn() }

    const handled = await new RemoteClientHost('/nonexistent-bundle').handleRequest(
      req as unknown as IncomingMessage,
      res as unknown as ServerResponse,
    )

    expect(handled).toBe(true)
    expect(res.writeHead).toHaveBeenCalledWith(400)
    expect(res.end).toHaveBeenCalled()
  })
})
