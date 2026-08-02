import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { addToast, showUrlToast, dismissToast, toastState } from './toast.svelte'

describe('toast slot', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    dismissToast()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('auto-dismisses a transient toast after 4 s', () => {
    addToast('Deploy: build queued on next', 'success')
    expect(toastState.visible).toBe(true)
    vi.advanceTimersByTime(4000)
    expect(toastState.visible).toBe(false)
  })

  it('keeps a sticky toast up with no timer', () => {
    addToast('Stopped watching Deploy #42 — lost contact with TeamCity', 'default', {
      sticky: true,
    })
    vi.advanceTimersByTime(60_000)
    expect(toastState.visible).toBe(true)
    dismissToast()
    expect(toastState.visible).toBe(false)
  })

  it('folds a transient message into a visible sticky toast instead of dropping it', () => {
    addToast('Stopped watching Deploy #42 — lost contact with TeamCity', 'default', {
      sticky: true,
    })
    addToast('Deploy: build queued on next', 'success')
    expect(toastState.message).toBe(
      'Deploy: build queued on next · Stopped watching Deploy #42 — lost contact with TeamCity',
    )
    // The fold must not arm a timer — the slot stays sticky.
    vi.advanceTimersByTime(10_000)
    expect(toastState.visible).toBe(true)
  })

  it('replaces a sticky toast with a newer sticky one', () => {
    addToast('give-up A', 'default', { sticky: true })
    addToast('give-up B', 'default', { sticky: true })
    expect(toastState.message).toBe('give-up B')
  })

  it('releases the slot after dismissal', () => {
    addToast('give-up', 'default', { sticky: true })
    dismissToast()
    addToast('Deploy: build queued on next', 'success')
    expect(toastState.message).toBe('Deploy: build queued on next')
    expect(toastState.kind).toBe('success')
  })

  it('guards the slot against passive URL toasts but lets a user click force it', () => {
    addToast('give-up', 'default', { sticky: true })
    showUrlToast('https://example.com')
    expect(toastState.message).toBe('give-up')
    expect(toastState.url).toBe('')
    showUrlToast('https://example.com', { force: true })
    expect(toastState.url).toBe('https://example.com')
    // Forcing hands the slot back to normal URL-toast semantics (8 s timer).
    vi.advanceTimersByTime(8000)
    expect(toastState.visible).toBe(false)
  })
})
