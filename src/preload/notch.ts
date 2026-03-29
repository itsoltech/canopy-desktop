import { contextBridge, ipcRenderer } from 'electron'
import type { IpcRendererEvent } from 'electron'

const notchApi = {
  onStateUpdate: (
    callback: (state: {
      sessions: Array<{
        ptySessionId: string
        windowId: number
        workspaceName: string
        branch: string | null
        status: string
        toolName?: string
        detail?: string
        title?: string
      }>
      notchWidth: number
      notchHeight: number
      peekSessionIds?: string[]
    }) => void,
  ) => {
    const handler = (_event: IpcRendererEvent, state: Parameters<typeof callback>[0]): void =>
      callback(state)
    ipcRenderer.on('notch:stateUpdate', handler)
    return (): void => {
      ipcRenderer.removeListener('notch:stateUpdate', handler)
    }
  },

  focusSession: (windowId: number, ptySessionId: string) =>
    ipcRenderer.invoke('notch:focusSession', { windowId, ptySessionId }),

  /** Toggle click-through on the overlay window (fire-and-forget). */
  setMouseIgnore: (ignore: boolean) => ipcRenderer.send('notch:setMouseIgnore', { ignore }),
}

if (process.contextIsolated) {
  contextBridge.exposeInMainWorld('notchApi', notchApi)
} else {
  // @ts-ignore -- fallback for non-isolated context
  window.notchApi = notchApi
}
