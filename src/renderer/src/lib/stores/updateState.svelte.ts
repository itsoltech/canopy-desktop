export type UpdateStatus =
  | 'idle'
  | 'up-to-date'
  | 'available'
  | 'downloading'
  | 'ready'
  | 'installing'
  | 'error'

interface UpdateState {
  status: UpdateStatus
  version: string
  percent: number
  errorMessage: string
  dismissed: boolean
}

export const updateState: UpdateState = $state({
  status: 'idle',
  version: '',
  percent: 0,
  errorMessage: '',
  dismissed: false,
})

let autoDismissTimer: ReturnType<typeof setTimeout> | null = null

function clearAutoDismiss(): void {
  if (autoDismissTimer) {
    clearTimeout(autoDismissTimer)
    autoDismissTimer = null
  }
}

export function initUpdateListeners(): () => void {
  const unsubs = [
    window.api.onUpdateAvailable((data) => {
      updateState.version = data.version
      updateState.status = 'available'
      updateState.dismissed = false
    }),
    window.api.onUpdateProgress((data) => {
      updateState.percent = Math.round(data.percent)
      updateState.status = 'downloading'
    }),
    window.api.onUpdateDownloaded((data) => {
      updateState.version = data.version
      updateState.status = 'ready'
      updateState.dismissed = false
    }),
    window.api.onUpdateNotAvailable(() => {
      updateState.status = 'up-to-date'
      updateState.dismissed = false
      clearAutoDismiss()
      autoDismissTimer = setTimeout(() => {
        if (updateState.status === 'up-to-date') updateState.status = 'idle'
        autoDismissTimer = null
      }, 4000)
    }),
    window.api.onUpdateError((data) => {
      updateState.errorMessage = data.message
      updateState.status = 'error'
      updateState.dismissed = false
      clearAutoDismiss()
      autoDismissTimer = setTimeout(() => {
        if (updateState.status === 'error') updateState.status = 'idle'
        autoDismissTimer = null
      }, 5000)
    }),
    window.api.onUpdateInstalling(() => {
      updateState.status = 'installing'
    }),
  ]

  return () => {
    unsubs.forEach((fn) => fn())
    clearAutoDismiss()
  }
}

export function installUpdate(): void {
  window.api.installUpdate()
}

export function dismissUpdate(): void {
  updateState.dismissed = true
}
