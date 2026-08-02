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

export function showUrlToast(url: string, opts?: { force?: boolean }): void {
  // Same slot guard as addToast: a page-initiated open (BrowserPane's
  // onBrowserOpenUrl) fires with the user away — exactly the case sticky exists
  // for. Direct user actions pass force to win the slot.
  if (sticky && !opts?.force) return
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
    toastState.message = `${message} · ${toastState.message}`
    return
  }
  if (dismissTimer) clearTimeout(dismissTimer)
  dismissTimer = null
  sticky = opts?.sticky ?? false
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

/** True while a sticky toast holds the slot — the CI give-up aggregation keys on it. */
export function isStickyToastVisible(): boolean {
  return sticky && toastState.visible
}

export function dismissToast(): void {
  if (dismissTimer) {
    clearTimeout(dismissTimer)
    dismissTimer = null
  }
  sticky = false
  toastState.visible = false
}
