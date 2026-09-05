export interface DelayedAction {
  readonly pending: boolean
  schedule: () => void
  cancel: () => void
  dispose: () => void
}

export function createDelayedAction(callback: () => void, delayMs: number): DelayedAction {
  let timer: ReturnType<typeof setTimeout> | null = null
  let active = true

  const cancel = (): void => {
    if (timer !== null) clearTimeout(timer)
    timer = null
  }

  return {
    get pending() {
      return timer !== null
    },
    schedule() {
      if (!active) return
      cancel()
      timer = setTimeout(() => {
        timer = null
        if (active) callback()
      }, delayMs)
    },
    cancel,
    dispose() {
      active = false
      cancel()
    },
  }
}
