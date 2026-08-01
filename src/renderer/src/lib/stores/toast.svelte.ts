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

export function showUrlToast(url: string): void {
  if (dismissTimer) clearTimeout(dismissTimer)
  toastState.url = url
  toastState.message = ''
  toastState.kind = 'default'
  toastState.visible = true
  dismissTimer = setTimeout(() => {
    dismissToast()
  }, 8000)
}

export function addToast(message: string, kind: ToastKind = 'default'): void {
  if (dismissTimer) clearTimeout(dismissTimer)
  toastState.message = message
  toastState.url = ''
  toastState.kind = kind
  toastState.visible = true
  dismissTimer = setTimeout(() => {
    dismissToast()
  }, 4000)
}

export function dismissToast(): void {
  if (dismissTimer) {
    clearTimeout(dismissTimer)
    dismissTimer = null
  }
  toastState.visible = false
}
