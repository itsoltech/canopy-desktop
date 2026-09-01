interface ToastState {
  visible: boolean
  url: string
  message: string
}

interface QueuedToast {
  url: string
  message: string
  durationMs: number
}

export const toastState: ToastState = $state({ visible: false, url: '', message: '' })

// Only one toast is on screen at a time. Without a queue each call overwrote the
// single slot, so a burst silently dropped everything but the last message —
// `for (const w of warnings) addToast(w)` showed only the final warning, and a
// warning immediately followed by a success toast was never seen at all.
const MAX_QUEUED = 10
const queue: QueuedToast[] = []
let dismissTimer: ReturnType<typeof setTimeout> | null = null

function enqueue(toast: QueuedToast): void {
  // Bounded so a runaway caller can't grow the backlog without limit; once it is
  // this deep the oldest pending toast is the least worth showing.
  if (queue.length >= MAX_QUEUED) queue.shift()
  queue.push(toast)
  if (!toastState.visible) showNext()
}

function showNext(): void {
  if (dismissTimer) {
    clearTimeout(dismissTimer)
    dismissTimer = null
  }
  const next = queue.shift()
  if (!next) {
    toastState.visible = false
    return
  }
  toastState.url = next.url
  toastState.message = next.message
  toastState.visible = true
  dismissTimer = setTimeout(showNext, next.durationMs)
}

export function showUrlToast(url: string): void {
  enqueue({ url, message: '', durationMs: 8000 })
}

export function addToast(message: string): void {
  enqueue({ url: '', message, durationMs: 4000 })
}

/** Dismiss the visible toast and advance to the next queued one, if any. */
export function dismissToast(): void {
  showNext()
}
