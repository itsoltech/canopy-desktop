import { ipcMain } from 'electron'
import type { PtyManager } from '../pty/PtyManager'
import type { WsBridge } from '../pty/WsBridge'

export function registerIpcHandlers(ptyManager: PtyManager, wsBridge: WsBridge): void {
  ipcMain.handle(
    'pty:spawn',
    async (_event, options?: { cols?: number; rows?: number; cwd?: string }) => {
      const session = ptyManager.spawn(options)
      const wsUrl = await wsBridge.create(session.id, session.pty)
      return { sessionId: session.id, wsUrl }
    }
  )

  ipcMain.handle(
    'pty:resize',
    (_event, payload: { sessionId: string; cols: number; rows: number }) => {
      ptyManager.resize(payload.sessionId, payload.cols, payload.rows)
    }
  )

  ipcMain.handle('pty:kill', (_event, payload: { sessionId: string }) => {
    wsBridge.destroy(payload.sessionId)
    ptyManager.kill(payload.sessionId)
  })
}
