import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { toastState, dismissToast } from './toast.svelte'

// observeBuild is exercised through triggerCiBuild (its only entry point). The
// preload bridge is stubbed; the REAL toast store is used so these tests pin the
// full user-visible contract: completion toasts, give-up toasts, aggregation.
const api = {
  ciTrigger: vi.fn(),
  ciBuild: vi.fn(),
  ciStatus: vi.fn(async () => ({ configured: false, rows: [] })),
  ciJobsStatus: vi.fn(),
  ciTriggerJob: vi.fn(),
  ciRun: vi.fn(),
}
vi.stubGlobal('window', { api })

import {
  ciKey,
  getCiJobsState,
  getCiState,
  refreshCi,
  refreshCiJobs,
  resetCiObserversForTests,
  triggerCiBuild,
  triggerCiJob,
} from './ci.svelte'

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => (resolve = done))
  return { promise, resolve }
}

describe('repository-scoped CI status', () => {
  it('keeps concurrent repository and branch responses in their own state slots', async () => {
    const teamCityA = deferred<CiStatusResponse>()
    const teamCityB = deferred<CiStatusResponse>()
    const githubA = deferred<Awaited<ReturnType<typeof api.ciJobsStatus>>>()
    const githubB = deferred<Awaited<ReturnType<typeof api.ciJobsStatus>>>()
    api.ciStatus.mockImplementation((_repo, branch) =>
      branch === 'a' ? teamCityA.promise : teamCityB.promise,
    )
    api.ciJobsStatus.mockImplementation((_repo, ref) =>
      ref.name === 'a' ? githubA.promise : githubB.promise,
    )

    const teamCityARequest = refreshCi('repo-a', 'a')
    const teamCityBRequest = refreshCi('repo-b', 'b')
    const githubARequest = refreshCiJobs('repo-a', 'a')
    const githubBRequest = refreshCiJobs('repo-b', 'b')
    teamCityB.resolve({ configured: true, rows: [], error: 'B' })
    githubB.resolve([{ jobId: 'b', label: 'B', provider: 'github-actions', run: null }])
    await Promise.all([teamCityBRequest, githubBRequest])
    teamCityA.resolve({ configured: true, rows: [], error: 'A' })
    githubA.resolve([{ jobId: 'a', label: 'A', provider: 'github-actions', run: null }])
    await Promise.all([teamCityARequest, githubARequest])

    expect(getCiState(ciKey('repo-a', 'a')).response?.error).toBe('A')
    expect(getCiState(ciKey('repo-b', 'b')).response?.error).toBe('B')
    expect(getCiJobsState(ciKey('repo-a', 'a')).rows[0]?.jobId).toBe('a')
    expect(getCiJobsState(ciKey('repo-b', 'b')).rows[0]?.jobId).toBe('b')
  })

  it('bounds repository and branch status state retained in memory', async () => {
    api.ciStatus.mockResolvedValue({ configured: false, rows: [] })

    for (let index = 0; index <= 100; index += 1) {
      await refreshCi(`cache-repo-${index}`, `branch-${index}`)
    }

    expect(getCiState(ciKey('cache-repo-0', 'branch-0')).response).toBeNull()
    expect(getCiState(ciKey('cache-repo-100', 'branch-100')).response).toMatchObject({
      configured: false,
    })
  })

  it('keeps an empty GitHub jobs result settled during later polls', async () => {
    const first = deferred<Awaited<ReturnType<typeof api.ciJobsStatus>>>()
    const second = deferred<Awaited<ReturnType<typeof api.ciJobsStatus>>>()
    api.ciJobsStatus.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const key = ciKey('settled-empty-repo', 'next')

    const firstRequest = refreshCiJobs('settled-empty-repo', 'next')
    expect(getCiJobsState(key)).toMatchObject({ loading: true, settled: false, rows: [] })
    first.resolve([])
    await firstRequest
    expect(getCiJobsState(key)).toMatchObject({ loading: false, settled: true, rows: [] })

    const secondRequest = refreshCiJobs('settled-empty-repo', 'next')
    expect(getCiJobsState(key)).toMatchObject({ loading: true, settled: true, rows: [] })
    second.resolve([])
    await secondRequest
  })

  it('marks a failed GitHub jobs request as settled', async () => {
    api.ciJobsStatus.mockRejectedValueOnce(new Error('offline'))
    const key = ciKey('settled-error-repo', 'next')

    await refreshCiJobs('settled-error-repo', 'next')

    expect(getCiJobsState(key)).toMatchObject({
      loading: false,
      settled: true,
      rows: [],
      error: 'offline',
    })
  })

  it('keeps the previous polling error visible while a retry is in flight', async () => {
    api.ciJobsStatus.mockRejectedValueOnce(new Error('offline'))
    const retry = deferred<Awaited<ReturnType<typeof api.ciJobsStatus>>>()
    api.ciJobsStatus.mockReturnValueOnce(retry.promise)
    const key = ciKey('persistent-error-repo', 'next')

    await refreshCiJobs('persistent-error-repo', 'next')
    const retryRequest = refreshCiJobs('persistent-error-repo', 'next')

    expect(getCiJobsState(key)).toMatchObject({
      loading: true,
      settled: true,
      rows: [],
      error: 'offline',
    })

    retry.resolve([])
    await retryRequest
  })
})

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

  afterEach(() => {
    resetCiObserversForTests()
    vi.useRealTimers()
  })

  it('reports the queue with the branch TeamCity actually accepted', async () => {
    queuedTrigger(1)
    const failure = await triggerCiBuild('r', 'https://tc.example.test', 'Bt', 'next', 'Deploy')
    expect(failure).toBeNull()
    expect(toastState.message).toBe('Deploy: build queued on release/1')
    expect(toastState.kind).toBe('success')
  })

  it('returns the failure message instead of toasting (the dialog owns the surface)', async () => {
    api.ciTrigger.mockRejectedValueOnce(new Error('TeamCity API error 403: forbidden'))
    const failure = await triggerCiBuild('r', 'https://tc.example.test', 'Bt', 'next', 'Deploy')
    expect(failure).toBe('TeamCity API error 403: forbidden')
    expect(toastState.visible).toBe(false)
  })

  it('toasts the outcome when the observed build finishes', async () => {
    queuedTrigger(2)
    await triggerCiBuild('r', 'https://tc.example.test', 'Bt', 'next', 'Deploy')
    api.ciBuild.mockResolvedValue({ id: 2, number: '42', state: 'finished', status: 'SUCCESS' })
    await vi.advanceTimersByTimeAsync(10_000)
    expect(toastState.message).toBe('Deploy #42: build succeeded')
    expect(toastState.kind).toBe('success')
    // The observation stops with the outcome — no further polls.
    const calls = api.ciBuild.mock.calls.length
    await vi.advanceTimersByTimeAsync(30_000)
    expect(api.ciBuild.mock.calls.length).toBe(calls)
  })

  it('waits for a slow TeamCity request to settle before polling again', async () => {
    queuedTrigger(21)
    await triggerCiBuild('r', 'https://tc.example.test', 'Bt', 'next', 'Deploy')
    const slow = deferred<{ id: number; number: string; state: 'finished'; status: 'SUCCESS' }>()
    api.ciBuild.mockReturnValueOnce(slow.promise)

    await vi.advanceTimersByTimeAsync(10_000)
    expect(api.ciBuild).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(15_000)
    expect(api.ciBuild).toHaveBeenCalledTimes(1)

    slow.resolve({ id: 21, number: '21', state: 'finished', status: 'SUCCESS' })
    await Promise.resolve()
    expect(toastState.message).toBe('Deploy #21: build succeeded')
    await vi.advanceTimersByTimeAsync(20_000)
    expect(api.ciBuild).toHaveBeenCalledTimes(1)
  })

  it("renders TeamCity's ERROR outcome as a failure, not unknown", async () => {
    queuedTrigger(3)
    await triggerCiBuild('r', 'https://tc.example.test', 'Bt', 'next', 'Deploy')
    api.ciBuild.mockResolvedValue({ id: 3, number: '43', state: 'finished', status: 'ERROR' })
    await vi.advanceTimersByTimeAsync(10_000)
    expect(toastState.message).toBe('Deploy #43: build failed')
    expect(toastState.kind).toBe('danger')
  })

  it('gives up audibly after ~5 minutes of consecutive failures', async () => {
    queuedTrigger(4)
    await triggerCiBuild('r', 'https://tc.example.test', 'Bt', 'next', 'Deploy')
    api.ciBuild.mockRejectedValue(new Error('offline'))
    await vi.advanceTimersByTimeAsync(300_000)
    expect(toastState.message).toBe('Stopped watching Deploy - lost contact with TeamCity')
    // Sticky: no timer may take it down.
    await vi.advanceTimersByTimeAsync(60_000)
    expect(toastState.visible).toBe(true)
    dismissToast()
  })

  it('uses a wall-clock failure window when TeamCity requests are slow', async () => {
    queuedTrigger(41)
    await triggerCiBuild('r', 'https://tc.example.test', 'Bt', 'next', 'Deploy')
    api.ciBuild.mockImplementation(
      () =>
        new Promise((_resolve, reject) => {
          setTimeout(() => reject(new Error('timeout')), 15_000)
        }),
    )

    await vi.advanceTimersByTimeAsync(300_000)

    expect(toastState.message).toBe('Stopped watching Deploy - lost contact with TeamCity')
    expect(api.ciBuild.mock.calls.length).toBeLessThan(30)
    dismissToast()
  })

  it('aggregates simultaneous give-ups into one toast that still names the jobs', async () => {
    queuedTrigger(5)
    await triggerCiBuild('r', 'https://tc.example.test', 'Bt', 'next', 'Deploy A')
    queuedTrigger(6)
    await triggerCiBuild('r', 'https://tc.example.test', 'Bt2', 'next', 'Deploy B')
    api.ciBuild.mockRejectedValue(new Error('offline'))
    await vi.advanceTimersByTimeAsync(300_000)
    expect(toastState.message).toBe(
      'Stopped watching 2 builds - check TeamCity: Deploy A, Deploy B',
    )
    dismissToast()
  })

  it('observes equal numeric build ids independently across repositories and servers', async () => {
    queuedTrigger(77)
    await triggerCiBuild('repo-a', 'https://tc-a.example.test', 'Bt', 'next', 'Build A')
    queuedTrigger(77)
    await triggerCiBuild('repo-b', 'https://tc-b.example.test', 'Bt', 'next', 'Build B')
    api.ciBuild.mockResolvedValue({
      id: 77,
      number: '77',
      state: 'running',
      status: 'UNKNOWN',
    })

    await vi.advanceTimersByTimeAsync(10_000)

    expect(api.ciBuild).toHaveBeenCalledWith('repo-a', 'https://tc-a.example.test', 77)
    expect(api.ciBuild).toHaveBeenCalledWith('repo-b', 'https://tc-b.example.test', 77)
  })
})

describe('triggerCiJob + observeRun', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    dismissToast()
    if (toastState.visible) dismissToast()
  })

  afterEach(() => {
    resetCiObserversForTests()
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
      ok: true,
      value: {
        runId: '12345678901234567890',
        webUrl: 'https://github.com/run/123',
        ref: request.ref,
      },
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
    api.ciTriggerJob.mockRejectedValueOnce(
      new Error(
        "Error invoking remote method 'ci:triggerJob': Error: GitHub API error 422: rejected",
      ),
    )

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

    expect(failure).toEqual({
      kind: 'failure',
      code: 'CiIpcError',
      message: 'GitHub API error 422: rejected',
    })
    expect(api.ciTriggerJob).toHaveBeenCalledOnce()
    expect(toastState.visible).toBe(false)
  })

  it('uses GitHub Actions wording when multiple workflow observations give up', async () => {
    api.ciTriggerJob
      .mockResolvedValueOnce({
        ok: true,
        value: {
          runId: '101',
          webUrl: 'https://github.com/run/101',
          ref: { name: 'next', kind: 'branch' },
        },
      })
      .mockResolvedValueOnce({
        ok: true,
        value: {
          runId: '102',
          webUrl: 'https://github.com/run/102',
          ref: { name: 'next', kind: 'branch' },
        },
      })
    const request = {
      jobId: '.github/workflows/release.yml',
      ref: { name: 'next', kind: 'branch' as const },
      inputs: {},
    }
    await triggerCiJob('r', request, 'Release')
    await triggerCiJob('r', { ...request, jobId: '.github/workflows/ci.yml' }, 'CI')
    api.ciRun.mockRejectedValue(new Error('offline'))

    await vi.advanceTimersByTimeAsync(300_000)

    expect(toastState.message).toBe(
      'Stopped watching 2 workflows - check GitHub Actions: Release, CI',
    )
    dismissToast()
  })

  it('stops safely when the repository CI provider changes during observation', async () => {
    api.ciTriggerJob.mockResolvedValueOnce({
      ok: true,
      value: {
        runId: '42',
        webUrl: 'https://github.com/run/42',
        ref: { name: 'next', kind: 'branch' },
      },
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
      'Stopped watching Release - CI provider changed - check GitHub Actions',
    )
    expect(toastState.kind).toBe('default')
  })

  it('treats native-confirmation cancellation as a quiet cancellation', async () => {
    api.ciTriggerJob.mockResolvedValueOnce({
      ok: false,
      error: { code: 'CiDispatchCancelled', message: 'Workflow cancelled before dispatch' },
    })

    const failure = await triggerCiJob(
      'r',
      {
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next', kind: 'branch' },
        inputs: {},
      },
      'Release',
    )

    expect(failure).toEqual({ kind: 'cancelled' })
    expect(toastState.visible).toBe(false)
  })

  it('preserves a structured trigger status without parsing the message', async () => {
    api.ciTriggerJob.mockResolvedValueOnce({
      ok: false,
      error: { code: 'CiApiError', status: 502, message: 'Upstream said Forbidden' },
    })

    const failure = await triggerCiJob(
      'r',
      {
        jobId: '.github/workflows/release.yml',
        ref: { name: 'next', kind: 'branch' },
        inputs: {},
      },
      'Release',
    )

    expect(failure).toEqual({
      kind: 'failure',
      code: 'CiApiError',
      status: 502,
      message: 'Upstream said Forbidden',
    })
  })
})
