import { afterEach, describe, expect, it, vi } from 'vitest'
import { startSettledPoll } from './settledPoll'

function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void
  const promise = new Promise<void>((done) => (resolve = done))
  return { promise, resolve }
}

describe('startSettledPoll', () => {
  afterEach(() => vi.useRealTimers())

  it('waits for a slow request to settle before starting the interval', async () => {
    vi.useFakeTimers()
    const first = deferred()
    const task = vi
      .fn()
      .mockImplementationOnce(() => first.promise)
      .mockResolvedValue(undefined)
    const stop = startSettledPoll(task, 10_000)

    expect(task).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(15_000)
    expect(task).toHaveBeenCalledTimes(1)

    first.resolve()
    await Promise.resolve()
    await vi.advanceTimersByTimeAsync(9_999)
    expect(task).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(1)
    expect(task).toHaveBeenCalledTimes(2)

    stop()
  })
})
