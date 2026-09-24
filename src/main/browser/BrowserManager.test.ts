import { describe, expect, it, vi } from 'vitest'
import type { BrowserWindow, WebContents } from 'electron'
import { BrowserManager } from './BrowserManager'

// Pins the OWNERSHIP contract of the browser <webview> channels: `entries` is a
// process-global map keyed by a renderer-supplied `browserId`, so a browserId
// alone must never authorize an operation. Without this guard any window could
// name a sibling window's browserId on browser:fillCredential and have stored
// credentials typed into a page it does not own.

const electronMock = vi.hoisted(() => ({
  webContents: { fromId: vi.fn(), getAllWebContents: vi.fn(() => [] as unknown[]) },
  WebContentsView: class {},
  Menu: { buildFromTemplate: vi.fn(() => ({ popup: vi.fn() })) },
  dialog: { showMessageBoxSync: vi.fn(() => 1) },
  session: {
    fromPartition: vi.fn(() => ({
      setPermissionRequestHandler: vi.fn(),
      setPermissionCheckHandler: vi.fn(),
      setDevicePermissionHandler: vi.fn(),
    })),
  },
}))

vi.mock('electron', () => electronMock)

/**
 * Stand-in for a <webview> guest WebContents. setup() wires a dozen listeners
 * onto the guest and teardown() probes DevTools/debugger state; a Proxy answers
 * everything else with no-ops so the test stays pinned to the ownership
 * contract rather than to those internals.
 */
function fakeGuest(id: number): WebContents {
  const nested: Record<string, unknown> = {
    // Object-valued properties must be modelled explicitly — the catch-all
    // below returns a function, which would break `wc.debugger.isAttached()`.
    debugger: { isAttached: () => false, attach: () => undefined, detach: () => undefined },
    session: { fetch: () => Promise.resolve({ ok: false }) },
  }
  return new Proxy(
    { id },
    {
      get(target, prop) {
        if (prop === 'id') return target.id
        if (prop === 'getType') return () => 'webview'
        if (prop === 'isDestroyed') return () => false
        if (prop === 'isDevToolsOpened') return () => false
        if (typeof prop === 'string' && prop in nested) return nested[prop]
        return () => undefined
      },
    },
  ) as unknown as WebContents
}

function fakeSender(id: number): WebContents {
  return { id, isDestroyed: () => false, send: vi.fn() } as unknown as WebContents
}

describe('BrowserManager ownership', () => {
  it('only reports ownership for the webContents that registered the browserId', () => {
    const manager = new BrowserManager()
    const owner = fakeSender(1)
    const other = fakeSender(2)
    electronMock.webContents.fromId.mockReturnValue(fakeGuest(42))

    manager.setup('browser-a', 42, {} as BrowserWindow, owner)

    expect(manager.ownedBy('browser-a', owner)).toBe(true)
    expect(manager.ownedBy('browser-a', other)).toBe(false)
  })

  it('reports no ownership for an unknown browserId', () => {
    const manager = new BrowserManager()
    expect(manager.ownedBy('never-registered', fakeSender(1))).toBe(false)
  })

  it('stops reporting ownership once the entry is torn down', () => {
    const manager = new BrowserManager()
    const owner = fakeSender(1)
    electronMock.webContents.fromId.mockReturnValue(fakeGuest(43))

    manager.setup('browser-b', 43, {} as BrowserWindow, owner)
    expect(manager.ownedBy('browser-b', owner)).toBe(true)

    manager.teardown('browser-b')
    expect(manager.ownedBy('browser-b', owner)).toBe(false)
  })
})
