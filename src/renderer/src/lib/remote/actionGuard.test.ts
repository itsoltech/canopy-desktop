import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../stores/preferences.svelte', () => ({ prefs: { 'remote.actionGuard': 'full' } }))
const confirm = vi.fn(async () => false)
vi.mock('../stores/dialogs.svelte', () => ({ confirm: () => confirm() }))

import { checkAction } from './actionGuard'

describe('checkAction under the full profile', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    confirm.mockClear()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it.each([
    ['diag.ping', { n: 1 }],
    ['pty.getDimensions', { sessionId: 's1' }],
    ['pty.unsubscribe', { sessionId: 's1' }],
  ] as const)('runs %s without a desktop prompt', async (method, params) => {
    await expect(checkAction(method, params)).resolves.toBe(true)
    expect(confirm).not.toHaveBeenCalled()
  })

  it('still asks before a host action', async () => {
    await expect(checkAction('tabs.activate', { tabId: 't1' })).resolves.toBe(false)
    expect(confirm).toHaveBeenCalledOnce()
  })
})
