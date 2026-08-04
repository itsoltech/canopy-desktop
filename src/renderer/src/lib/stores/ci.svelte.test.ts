import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { toastState, dismissToast } from './toast.svelte'

// observeBuild is exercised through triggerCiBuild (its only entry point). The
// preload bridge is stubbed; the REAL toast store is used so these tests pin the
// full user-visible contract: completion toasts, give-up toasts, aggregation.
const api = {
  ciTrigger: vi.fn(),
  ciBuild: vi.fn(),
  ciStatus: vi.fn(async () => ({ configured: false, rows: [] })),
  ciTriggerJob: vi.fn(),
  ciRun: vi.fn(),
}
vi.stubGlobal('window', { api })

import { triggerCiBuild, triggerCiJob } from './ci.svelte'

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
    api.ciRun.mockResolvedValue({
      provider: 'github-actions',
      runId: '0',
      number: '0',
      jobId: 'workflow.yml',
      jobLabel: 'Workflow',
      state: 'finished',
      conclusion: 'unknown',
      statusText: undefined,
      webUrl: 'https://github.com/run/0',
      ref: undefined,
      queuedAt: undefined,
      startedAt: undefined,
      finishedAt: undefined,
    })
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

describe('triggerCiJob + observeRun', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    dismissToast()
    if (toastState.visible) dismissToast()
  })

  afterEach(async () => {
    api.ciRun.mockResolvedValue({
      provider: 'github-actions',
      runId: '0',
      number: '0',
      jobId: 'workflow.yml',
      jobLabel: 'Workflow',
      state: 'finished',
      conclusion: 'unknown',
      statusText: undefined,
      webUrl: 'https://github.com/run/0',
      ref: undefined,
      queuedAt: undefined,
      startedAt: undefined,
      finishedAt: undefined,
    })
    await vi.advanceTimersByTimeAsync(10_000)
    vi.useRealTimers()
  })

  it('watches the exact string run id and reports a cancelled conclusion', async () => {
    const request = {
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' as const },
      schemaRevision: 'blob-sha',
      inputs: { dry_run: true },
    }
    api.ciTriggerJob.mockResolvedValueOnce({
      runId: '12345678901234567890',
      webUrl: 'https://github.com/run/123',
      ref: request.ref,
    })

    const failure = await triggerCiJob('r', request, 'Release')

    expect(failure).toBeNull()
    expect(api.ciTriggerJob).toHaveBeenCalledOnce()
    expect(api.ciTriggerJob).toHaveBeenCalledWith('r', request)
    expect(toastState.message).toBe('Release: workflow queued on next')

    api.ciRun.mockResolvedValueOnce({
      provider: 'github-actions',
      runId: '12345678901234567890',
      number: '77',
      jobId: request.jobId,
      jobLabel: 'Release',
      state: 'finished',
      conclusion: 'cancelled',
      statusText: 'Cancelled by user',
      webUrl: 'https://github.com/run/123',
      ref: request.ref,
      queuedAt: undefined,
      startedAt: undefined,
      finishedAt: undefined,
    })
    await vi.advanceTimersByTimeAsync(10_000)

    expect(api.ciRun).toHaveBeenCalledWith('r', '12345678901234567890')
    expect(toastState.message).toBe('Release #77: workflow cancelled')
  })

  it('does not retry a rejected dispatch', async () => {
    api.ciTriggerJob.mockRejectedValueOnce(new Error('GitHub API error 422: rejected'))

    const failure = await triggerCiJob(
      'r',
      {
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next', kind: 'branch' },
        schemaRevision: 'blob-sha',
        inputs: {},
      },
      'Release',
    )

    expect(failure).toBe('GitHub API error 422: rejected')
    expect(api.ciTriggerJob).toHaveBeenCalledOnce()
    expect(toastState.visible).toBe(false)
  })

  it('stops safely when the repository CI provider changes during observation', async () => {
    api.ciTriggerJob.mockResolvedValueOnce({
      runId: '42',
      webUrl: 'https://github.com/run/42',
      ref: { name: 'next', kind: 'branch' },
    })
    await triggerCiJob(
      'r',
      {
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next', kind: 'branch' },
        schemaRevision: 'blob-sha',
        inputs: {},
      },
      'Release',
    )
    api.ciRun.mockResolvedValueOnce({
      provider: 'teamcity',
      runId: '42',
      number: '42',
      jobId: 'Build',
      jobLabel: 'Build',
      state: 'finished',
      conclusion: 'success',
      statusText: 'Different provider',
      webUrl: 'https://teamcity/build/42',
      ref: { name: 'next', kind: 'branch' },
      queuedAt: undefined,
      startedAt: undefined,
      finishedAt: undefined,
    })

    await vi.advanceTimersByTimeAsync(10_000)

    expect(toastState.message).toBe(
      'Stopped watching Release — CI provider changed — check GitHub Actions',
    )
    expect(toastState.kind).toBe('default')
  })

  it('treats native-confirmation cancellation as a quiet cancellation', async () => {
    api.ciTriggerJob.mockRejectedValueOnce(new Error('Workflow cancelled before dispatch'))

    const failure = await triggerCiJob(
      'r',
      {
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next', kind: 'branch' },
        inputs: {},
      },
      'Release',
    )

    expect(failure).toBeNull()
    expect(toastState.visible).toBe(false)
  })
})
