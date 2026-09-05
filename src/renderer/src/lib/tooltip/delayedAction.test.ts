import { afterEach, describe, expect, it, vi } from 'vitest'
import { createDelayedAction } from './delayedAction'

afterEach(() => vi.useRealTimers())

describe('createDelayedAction', () => {
  it('replaces a pending timer and can dismiss both mouse and focus scheduling', () => {
    vi.useFakeTimers()
    const callback = vi.fn()
    const action = createDelayedAction(callback, 400)

    action.schedule()
    action.schedule()
    action.cancel()
    vi.advanceTimersByTime(400)

    expect(callback).not.toHaveBeenCalled()
  })

  it('cannot fire after its owner is disposed', () => {
    vi.useFakeTimers()
    const callback = vi.fn()
    const action = createDelayedAction(callback, 400)

    action.schedule()
    action.dispose()
    vi.advanceTimersByTime(400)

    expect(callback).not.toHaveBeenCalled()
  })
})
