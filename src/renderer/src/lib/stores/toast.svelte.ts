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

export function showUrlToast(url: string): void {
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
  // Sticky-over-sticky still replaces (the newer failure is at least as relevant);
  // auto-dismissing chatter waits for the ✕ / Escape.
  if (sticky && !opts?.sticky) return
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

export function dismissToast(): void {
  if (dismissTimer) {
    clearTimeout(dismissTimer)
    dismissTimer = null
  }
  sticky = false
  toastState.visible = false
}
