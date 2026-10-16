import { afterEach, describe, expect, it, vi } from 'vitest'

// The real module reads `app.isPackaged` at import time, which needs an Electron runtime.
vi.mock('@electron-toolkit/utils', () => ({ is: { dev: false } }))

import { AgentHookRouter } from './AgentHookServer'

describe('AgentHookRouter', () => {
  let router: AgentHookRouter | null = null

  afterEach(() => {
    router?.dispose()
    router = null
  })

  it('answers a session id with a malformed percent-escape with 400', async () => {
    router = new AgentHookRouter()
    const { port } = await router.addSession(
      'session-1',
      () => undefined,
      () => undefined,
    )

    const response = await fetch(`http://127.0.0.1:${port}/session/%E0%A4%A/hook`, {
      method: 'POST',
      body: '{}',
      signal: AbortSignal.timeout(5_000),
    })

    expect(response.status).toBe(400)
  })
})
