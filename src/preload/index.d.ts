import type { ElectronAPI } from '@electron-toolkit/preload'

interface PtySpawnResult {
  sessionId: string
  wsUrl: string
}

interface NixttyAPI {
  spawnPty: (options?: { cols?: number; rows?: number; cwd?: string }) => Promise<PtySpawnResult>
  resizePty: (sessionId: string, cols: number, rows: number) => Promise<void>
  killPty: (sessionId: string) => Promise<void>
}

declare global {
  interface Window {
    electron: ElectronAPI
    api: NixttyAPI
  }
}
