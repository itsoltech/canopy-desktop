import { afterEach, describe, expect, it } from 'vitest'
import { SignalingServer, type SignalingServerHandlers } from './SignalingServer'

const handlers: SignalingServerHandlers = {
  onPairAttempt: () => ({ ok: false, reason: 'not under test' }),
  onPeerSignal: () => {},
  onPeerDisconnected: () => {},
}

describe('SignalingServer.start', () => {
  const server = new SignalingServer()

  afterEach(async () => {
    await server.stop()
  })

  it('shares one listener between overlapping start calls', async () => {
    const opts = { bundleRoot: '/nonexistent-bundle', handlers, bindHost: '127.0.0.1' }

    const [first, second] = await Promise.all([server.start(opts), server.start(opts)])

    expect(first._unsafeUnwrap().port).toBe(server.listeningPort)
    expect(second._unsafeUnwrap().port).toBe(server.listeningPort)
  })
})
