export type ToastKind = 'default' | 'success' | 'danger'

interface ToastState {
  visible: boolean
  url: string
  message: string
  kind: ToastKind
}

export const toastState: ToastState = $state({
  visible: false,
  url: '',
  message: '',
  kind: 'default',
})

let dismissTimer: ReturnType<typeof setTimeout> | null = null
// Guards the single slot: while a sticky toast is visible, transient chatter must
// not silently take its place — it is the ONLY surface its state has.
let sticky = false
// A sticky message displaced by a forced URL toast — restored when that toast
// goes away, because clicking a link is not acknowledging a CI hand-off.
let stashedSticky: { message: string; kind: ToastKind } | null = null

export function showUrlToast(url: string, opts?: { force?: boolean }): void {
  // Same slot guard as addToast: a page-initiated open (BrowserPane's
  // onBrowserOpenUrl) fires with the user away — exactly the case sticky exists
  // for. Direct user actions pass force to win the slot — temporarily: the
  // sticky message is stashed and comes back when the URL toast is gone.
  if (sticky && !opts?.force) return
  if (sticky) stashedSticky = { message: toastState.message, kind: toastState.kind }
  if (dismissTimer) clearTimeout(dismissTimer)
  sticky = false
  toastState.url = url
  toastState.message = ''
  toastState.kind = 'default'
  toastState.visible = true
  dismissTimer = setTimeout(() => {
    dismissToast()
  }, 8000)
}

export function addToast(
  message: string,
  kind: ToastKind = 'default',
  opts?: { sticky?: boolean },
): void {
  // Sticky-over-sticky replaces (the newer failure is at least as relevant). A
  // transient toast must not take the slot — but it must not vanish either (the
  // queue confirmation and other builds' outcomes arrive this way): fold it in
  // front, where the ~40 visible chars are; the full text lives in the title.
  if (sticky && !opts?.sticky) {
    // Bounded: the slot can be held for hours and background call sites keep
    // feeding it — keep the 2 newest transients (deduped) with the sticky text
    // always last, or the title and the live-region mirror grow into a paragraph.
    const parts = toastState.message.split(' · ')
    const held = parts.pop()!
    if (parts[0] !== message) {
      toastState.message = [message, ...parts.slice(0, 1), held].join(' · ')
    }
    // Escalate the chrome: kind is this component's only state signal, and the
    // sticky holder is 'default' — keeping it would render a red failure neutral.
    if (kind !== 'default') toastState.kind = kind
    return
  }
  if (dismissTimer) clearTimeout(dismissTimer)
  dismissTimer = null
  sticky = opts?.sticky ?? false
  if (sticky) stashedSticky = null
  toastState.message = message
  toastState.url = ''
  toastState.kind = kind
  toastState.visible = true
  // Sticky: no auto-dismiss. For states with no other surface in the app, 4 s only
  // ever reaches a user who is already looking.
  if (!opts?.sticky) {
    dismissTimer = setTimeout(() => {
      dismissToast()
    }, 4000)
  }
}

/**
 * True while a sticky toast holds the slot (or waits stashed behind a forced URL
 * toast) — the CI give-up aggregation keys on it.
 */
export function isStickyToastVisible(): boolean {
  return (sticky && toastState.visible) || stashedSticky !== null
}

export function dismissToast(): void {
  if (dismissTimer) {
    clearTimeout(dismissTimer)
    dismissTimer = null
  }
  // A stashed sticky message returns instead of the slot closing — dismissing the
  // URL toast that displaced it is not acknowledging the hand-off it carries.
  if (!sticky && stashedSticky) {
    toastState.message = stashedSticky.message
    toastState.kind = stashedSticky.kind
    toastState.url = ''
    toastState.visible = true
    sticky = true
    stashedSticky = null
    return
  }
  sticky = false
  toastState.visible = false
}
