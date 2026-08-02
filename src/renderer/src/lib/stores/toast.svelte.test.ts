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

  it('escalates the chrome when a danger toast folds into a sticky default one', () => {
    addToast('Stopped watching Deploy #42 — lost contact with TeamCity', 'default', {
      sticky: true,
    })
    addToast('Deploy #17: build failed', 'danger')
    expect(toastState.kind).toBe('danger')
    expect(toastState.message.startsWith('Deploy #17: build failed')).toBe(true)
  })

  it('keeps the most severe kind when a success folds in after a failure', () => {
    // Newest-wins would paint the slot green while the (truncated) failure is
    // still inside the folded message.
    addToast('give-up', 'default', { sticky: true })
    addToast('Deploy A #17: build failed', 'danger')
    addToast('Deploy B #18: build succeeded', 'success')
    expect(toastState.kind).toBe('danger')
  })

  it('drops the escalated kind once the segment that earned it is evicted', () => {
    // A red slot whose message (and title) no longer mentions any failure would
    // be the inverse of the green-slot-carrying-a-failure mismatch.
    addToast('give-up', 'default', { sticky: true })
    addToast('Deploy #17: build failed', 'danger')
    addToast('post_run one')
    addToast('post_run two')
    expect(toastState.message).toBe('post_run two · post_run one · give-up')
    expect(toastState.kind).toBe('default')
  })

  it('caps the fold at the 2 newest transients and dedupes an identical repeat', () => {
    addToast('sticky-tail', 'default', { sticky: true })
    addToast('post_run one')
    addToast('post_run one') // identical repeat — must not stack
    addToast('post_run two')
    addToast('post_run three')
    expect(toastState.message).toBe('post_run three · post_run two · sticky-tail')
  })

  it('guards the slot against passive URL toasts but lets a user click force it', () => {
    addToast('give-up', 'default', { sticky: true })
    showUrlToast('https://example.com')
    expect(toastState.message).toBe('give-up')
    expect(toastState.url).toBe('')
    showUrlToast('https://example.com', { force: true })
    expect(toastState.url).toBe('https://example.com')
    // The forced URL toast only DISPLACES the sticky message: when its 8 s timer
    // fires, the give-up comes back — a link click is not an acknowledgement.
    vi.advanceTimersByTime(8000)
    expect(toastState.visible).toBe(true)
    expect(toastState.message).toBe('give-up')
    expect(toastState.url).toBe('')
    // And the restored toast is sticky again: no timer, transient folds in.
    vi.advanceTimersByTime(60_000)
    expect(toastState.visible).toBe(true)
    addToast('queued')
    expect(toastState.message).toBe('queued · give-up')
    dismissToast()
    expect(toastState.visible).toBe(false)
  })

  it('drops the stash when a newer sticky replaces the slot', () => {
    addToast('give-up A', 'default', { sticky: true })
    showUrlToast('https://example.com', { force: true })
    addToast('give-up B', 'default', { sticky: true })
    dismissToast()
    // B was acknowledged and A must not resurrect — sticky-over-sticky replaced it.
    expect(toastState.visible).toBe(false)
  })
})
