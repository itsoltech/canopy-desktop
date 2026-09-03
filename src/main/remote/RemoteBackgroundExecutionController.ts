import type { RemoteSessionStatus } from './types'

export interface PowerSaveBlockerLike {
  start(type: 'prevent-app-suspension'): number
  stop(id: number): boolean
  isStarted(id: number): boolean
}

export interface BackgroundThrottlingTarget {
  isDestroyed(): boolean
  getBackgroundThrottling(): boolean
  setBackgroundThrottling(allowed: boolean): void
}

interface RemoteBackgroundExecutionControllerOptions {
  powerSaveBlocker: PowerSaveBlockerLike
  findWebContents: (id: number) => BackgroundThrottlingTarget | null
}

export function remoteSessionNeedsBackgroundExecution(status: RemoteSessionStatus): boolean {
  return status.kind !== 'idle' && status.kind !== 'error'
}

/**
 * Keeps the signaling server and its renderer-side WebRTC/data forwarding alive
 * while Remote Control is listening or connected. The display may still turn
 * off; explicit system sleep is handled separately by powerMonitor.
 */
export class RemoteBackgroundExecutionController {
  private blockerId: number | null = null
  private unthrottledWebContents: { id: number; previousAllowed: boolean } | null = null

  constructor(private options: RemoteBackgroundExecutionControllerOptions) {}

  sync(status: RemoteSessionStatus, hostWebContentsId: number | null): void {
    const active = remoteSessionNeedsBackgroundExecution(status)
    this.syncPowerSaveBlocker(active)
    this.syncBackgroundThrottling(active ? hostWebContentsId : null)
  }

  dispose(): void {
    this.syncPowerSaveBlocker(false)
    this.syncBackgroundThrottling(null)
  }

  private syncPowerSaveBlocker(active: boolean): void {
    if (this.blockerId !== null && !this.options.powerSaveBlocker.isStarted(this.blockerId)) {
      this.blockerId = null
    }

    if (active && this.blockerId === null) {
      this.blockerId = this.options.powerSaveBlocker.start('prevent-app-suspension')
      return
    }

    if (!active && this.blockerId !== null) {
      this.options.powerSaveBlocker.stop(this.blockerId)
      this.blockerId = null
    }
  }

  private syncBackgroundThrottling(hostWebContentsId: number | null): void {
    if (this.unthrottledWebContents?.id === hostWebContentsId) return

    if (this.unthrottledWebContents !== null) {
      const { id, previousAllowed } = this.unthrottledWebContents
      const previous = this.options.findWebContents(id)
      if (previous && !previous.isDestroyed()) {
        previous.setBackgroundThrottling(previousAllowed)
      }
      this.unthrottledWebContents = null
    }

    if (hostWebContentsId === null) return
    const next = this.options.findWebContents(hostWebContentsId)
    if (!next || next.isDestroyed()) return
    const previousAllowed = next.getBackgroundThrottling()
    next.setBackgroundThrottling(false)
    this.unthrottledWebContents = { id: hostWebContentsId, previousAllowed }
  }
}
