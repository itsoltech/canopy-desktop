const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'

/**
 * Keep Tab cycling inside `container`. Call from a keydown handler when
 * `e.key === 'Tab'` — an `aria-modal` dialog whose Tab walks into the (visually
 * inert) page behind the scrim leaves stray Enter presses firing hidden controls.
 */
export function cycleFocus(container: HTMLElement, e: KeyboardEvent): void {
  const focusable = container.querySelectorAll<HTMLElement>(FOCUSABLE)
  if (focusable.length === 0) return
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  const active = document.activeElement as HTMLElement | null
  if (e.shiftKey && (active === first || !container.contains(active))) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && (active === last || !container.contains(active))) {
    e.preventDefault()
    first.focus()
  }
}

/** Capture the current opener and return a cleanup that restores it when it still exists. */
export function captureFocusReturn(): () => void {
  const previouslyFocused = document.activeElement as HTMLElement | null
  return () => {
    if (previouslyFocused?.isConnected) previouslyFocused.focus()
  }
}
