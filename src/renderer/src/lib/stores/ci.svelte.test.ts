import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { toastState, dismissToast } from './toast.svelte'

// observeBuild is exercised through triggerCiBuild (its only entry point). The
// preload bridge is stubbed; the REAL toast store is used so these tests pin the
// full user-visible contract: completion toasts, give-up toasts, aggregation.
const api = {
  ciTrigger: vi.fn(),
  ciBuild: vi.fn(),
  ciStatus: vi.fn(async () => ({ configured: false, rows: [] })),
}
vi.stubGlobal('window', { api })

import { triggerCiBuild } from './ci.svelte'

function queuedTrigger(buildId: number): void {
  api.ciTrigger.mockResolvedValueOnce({
    buildId,
    webUrl: `https://tc/build/${buildId}`,
    branchName: 'release/1',
  })
}

describe('triggerCiBuild + observeBuild', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    api.ciStatus.mockResolvedValue({ configured: false, rows: [] })
    dismissToast()
    if (toastState.visible) dismissToast() // a restored stash needs a second dismiss
  })

  afterEach(async () => {
    // Stop any observation still polling so it cannot leak into the next test.
    api.ciBuild.mockResolvedValue({ id: 0, number: '0', state: 'finished', status: 'UNKNOWN' })
    await vi.advanceTimersByTimeAsync(10_000)
    vi.useRealTimers()
  })

  it('reports the queue with the branch TeamCity actually accepted', async () => {
    queuedTrigger(1)
    const failure = await triggerCiBuild('r', 'Bt', 'next', 'Deploy')
    expect(failure).toBeNull()
    expect(toastState.message).toBe('Deploy: build queued on release/1')
    expect(toastState.kind).toBe('success')
  })

  it('returns the failure message instead of toasting (the dialog owns the surface)', async () => {
    api.ciTrigger.mockRejectedValueOnce(new Error('TeamCity API error 403: forbidden'))
    const failure = await triggerCiBuild('r', 'Bt', 'next', 'Deploy')
    expect(failure).toBe('TeamCity API error 403: forbidden')
    expect(toastState.visible).toBe(false)
  })

  it('toasts the outcome when the observed build finishes', async () => {
    queuedTrigger(2)
    await triggerCiBuild('r', 'Bt', 'next', 'Deploy')
    api.ciBuild.mockResolvedValue({ id: 2, number: '42', state: 'finished', status: 'SUCCESS' })
    await vi.advanceTimersByTimeAsync(10_000)
    expect(toastState.message).toBe('Deploy #42: build succeeded')
    expect(toastState.kind).toBe('success')
    // The observation stops with the outcome — no further polls.
    const calls = api.ciBuild.mock.calls.length
    await vi.advanceTimersByTimeAsync(30_000)
    expect(api.ciBuild.mock.calls.length).toBe(calls)
  })

  it("renders TeamCity's ERROR outcome as a failure, not unknown", async () => {
    queuedTrigger(3)
    await triggerCiBuild('r', 'Bt', 'next', 'Deploy')
    api.ciBuild.mockResolvedValue({ id: 3, number: '43', state: 'finished', status: 'ERROR' })
    await vi.advanceTimersByTimeAsync(10_000)
    expect(toastState.message).toBe('Deploy #43: build failed')
    expect(toastState.kind).toBe('danger')
  })

  it('gives up audibly after ~5 minutes of consecutive failures', async () => {
    queuedTrigger(4)
    await triggerCiBuild('r', 'Bt', 'next', 'Deploy')
    api.ciBuild.mockRejectedValue(new Error('offline'))
    await vi.advanceTimersByTimeAsync(300_000) // 30 ticks
    expect(toastState.message).toBe('Stopped watching Deploy — lost contact with TeamCity')
    // Sticky: no timer may take it down.
    await vi.advanceTimersByTimeAsync(60_000)
    expect(toastState.visible).toBe(true)
    dismissToast()
  })

  it('aggregates simultaneous give-ups into one toast that still names the jobs', async () => {
    queuedTrigger(5)
    await triggerCiBuild('r', 'Bt', 'next', 'Deploy A')
    queuedTrigger(6)
    await triggerCiBuild('r', 'Bt2', 'next', 'Deploy B')
    api.ciBuild.mockRejectedValue(new Error('offline'))
    await vi.advanceTimersByTimeAsync(300_000)
    expect(toastState.message).toBe(
      'Stopped watching 2 builds — check TeamCity: Deploy A, Deploy B',
    )
    dismissToast()
  })
})
