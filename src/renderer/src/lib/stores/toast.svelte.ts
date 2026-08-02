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

const KIND_RANK: Record<ToastKind, number> = { default: 0, success: 1, danger: 2 }

let dismissTimer: ReturnType<typeof setTimeout> | null = null
// Kinds of the transient segments currently folded into a sticky toast (newest
// first, same order and cap as the message segments) plus the sticky holder's own
// kind — the fold's cap evicts segments, and the chrome must describe what
// SURVIVES, so the kind is recomputed from the retained segments on every fold.
let foldedKinds: ToastKind[] = []
let stickyBaseKind: ToastKind = 'default'
// Guards the single slot: while a sticky toast is visible, transient chatter must
// not silently take its place — it is the ONLY surface its state has.
let sticky = false
// A sticky message displaced by a forced URL toast — restored when that toast
// goes away, because clicking a link is not acknowledging a CI hand-off. The
// message and the fold bookkeeping are ONE value: without foldedKinds and
// stickyBaseKind travelling along, a fold after the restore would recompute the
// kind against empty bookkeeping while the danger segment is still on screen.
let stashedSticky: {
  message: string
  kind: ToastKind
  foldedKinds: ToastKind[]
  stickyBaseKind: ToastKind
} | null = null

export function showUrlToast(url: string, opts?: { force?: boolean }): void {
  // Same slot guard as addToast: a page-initiated open (BrowserPane's
  // onBrowserOpenUrl) fires with the user away — exactly the case sticky exists
  // for. Direct user actions pass force to win the slot — temporarily: the
  // sticky message is stashed and comes back when the URL toast is gone.
  if (sticky && !opts?.force) return
  if (sticky) {
    stashedSticky = {
      message: toastState.message,
      kind: toastState.kind,
      foldedKinds: [...foldedKinds],
      stickyBaseKind,
    }
  }
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
    // (The split relies on sticky messages never containing the ' · ' separator —
    // reportGiveUp uses '—' and ',' only.)
    const parts = toastState.message.split(' · ')
    const held = parts.pop()!
    if (parts[0] === message) {
      // Deduped repeat — it may still raise its own segment's severity.
      if (KIND_RANK[kind] > KIND_RANK[foldedKinds[0] ?? 'default']) foldedKinds[0] = kind
    } else {
      toastState.message = [message, ...parts.slice(0, 1), held].join(' · ')
      foldedKinds = [kind, ...foldedKinds.slice(0, 1)]
    }
    // The chrome is recomputed from the RETAINED segments, not accumulated: a red
    // slot whose evicted failure no longer appears in the message (or the title)
    // would be the round-28 mismatch inverted. Kind is this component's only
    // state signal, so it must always describe text that is still on screen.
    toastState.kind = foldedKinds.reduce<ToastKind>(
      (a, b) => (KIND_RANK[b] > KIND_RANK[a] ? b : a),
      stickyBaseKind,
    )
    return
  }
  if (dismissTimer) clearTimeout(dismissTimer)
  dismissTimer = null
  sticky = opts?.sticky ?? false
  foldedKinds = []
  if (sticky) {
    stashedSticky = null
    stickyBaseKind = kind
  }
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
    foldedKinds = stashedSticky.foldedKinds
    stickyBaseKind = stashedSticky.stickyBaseKind
    toastState.url = ''
    toastState.visible = true
    sticky = true
    stashedSticky = null
    return
  }
  sticky = false
  foldedKinds = []
  toastState.visible = false
}
