import type http from 'node:http'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { RemoteClientHost } from './RemoteClientHost'

interface CapturedResponse {
  status: number | null
  ended: boolean
}

function fakeResponse(): { res: http.ServerResponse; captured: CapturedResponse } {
  const captured: CapturedResponse = { status: null, ended: false }
  const res = {
    writeHead(status: number) {
      captured.status = status
      return res
    },
    end() {
      captured.ended = true
      return res
    },
  }
  // Only the two members RemoteClientHost touches on these paths are needed.
  return { res: res as unknown as http.ServerResponse, captured }
}

function fakeRequest(url: string): http.IncomingMessage {
  // handleRequest reads only `url` and `method` before the paths under test respond.
  return { url, method: 'GET' } as http.IncomingMessage
}

describe('RemoteClientHost.handleRequest', () => {
  let bundleRoot: string

  beforeEach(() => {
    bundleRoot = mkdtempSync(join(tmpdir(), 'canopy-remote-bundle-'))
    writeFileSync(join(bundleRoot, 'remote.html'), '<!doctype html>')
  })

  afterEach(() => {
    rmSync(bundleRoot, { recursive: true, force: true })
  })

  it('answers 400 for a request target the URL parser rejects instead of rejecting', async () => {
    // `GET //% HTTP/1.1` passes Node's HTTP parser but is not a valid URL path.
    const host = new RemoteClientHost(bundleRoot)
    const { res, captured } = fakeResponse()

    await expect(host.handleRequest(fakeRequest('//%'), res)).resolves.toBe(true)
    expect(captured.status).toBe(400)
    expect(captured.ended).toBe(true)
  })

  it('leaves paths outside /remote to the caller', async () => {
    const host = new RemoteClientHost(bundleRoot)
    const { res, captured } = fakeResponse()

    await expect(host.handleRequest(fakeRequest('/health-check'), res)).resolves.toBe(false)
    expect(captured.status).toBeNull()
  })
})
