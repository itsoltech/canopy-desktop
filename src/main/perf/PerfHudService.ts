import { app, type WebContents } from 'electron'

const SAMPLE_INTERVAL_MS = 1000
const CHANNEL = 'perf:hud:metrics'

export interface PerfHudMetrics {
  cpu: number
  memMb: number
}

/**
 * Sampling loop for the status-bar perf HUD.
 *
 * Designed to add zero overhead when nobody is watching: the interval only runs
 * while at least one renderer is subscribed. When the last subscriber disconnects
 * (toggle off, window closed, reload) the interval is cleared.
 *
 * Sampling cost: one synchronous app.getAppMetrics() call per tick (~sub-ms even
 * with many child processes), small object allocation, optional structured clone
 * for IPC. Memoized last payload skips IPC + DOM updates when nothing changed.
 */
export class PerfHudService {
  private subscribers = new Map<number, WebContents>()
  private destroyHandlers = new Map<number, () => void>()
  private intervalHandle: ReturnType<typeof setInterval> | null = null
  private lastPayload: PerfHudMetrics | null = null

  subscribe(wc: WebContents): void {
    const id = wc.id
    if (this.subscribers.has(id)) return

    this.subscribers.set(id, wc)

    const onDestroyed = (): void => this.unsubscribeById(id)
    wc.once('destroyed', onDestroyed)
    this.destroyHandlers.set(id, onDestroyed)

    // Send last known sample immediately so the HUD doesn't show empty state
    // for up to a full second after toggling on.
    if (this.lastPayload) {
      wc.send(CHANNEL, this.lastPayload)
    }

    this.startIfNeeded()
  }

  unsubscribe(wc: WebContents): void {
    this.unsubscribeById(wc.id)
  }

  shutdown(): void {
    this.stopInterval()
    for (const [id, handler] of this.destroyHandlers) {
      const wc = this.subscribers.get(id)
      if (wc && !wc.isDestroyed()) wc.removeListener('destroyed', handler)
    }
    this.subscribers.clear()
    this.destroyHandlers.clear()
    this.lastPayload = null
  }

  private unsubscribeById(id: number): void {
    const wc = this.subscribers.get(id)
    if (!wc) return
    const handler = this.destroyHandlers.get(id)
    if (handler && !wc.isDestroyed()) wc.removeListener('destroyed', handler)
    this.subscribers.delete(id)
    this.destroyHandlers.delete(id)
    if (this.subscribers.size === 0) this.stopInterval()
  }

  private startIfNeeded(): void {
    if (this.intervalHandle) return
    if (this.subscribers.size === 0) return
    // Tick once immediately to populate state, then on interval.
    this.tick()
    this.intervalHandle = setInterval(() => this.tick(), SAMPLE_INTERVAL_MS)
  }

  private stopInterval(): void {
    if (!this.intervalHandle) return
    clearInterval(this.intervalHandle)
    this.intervalHandle = null
  }

  private tick(): void {
    if (this.subscribers.size === 0) {
      this.stopInterval()
      return
    }

    const metrics = app.getAppMetrics()
    let cpuTotal = 0
    let memKbTotal = 0
    for (const m of metrics) {
      cpuTotal += m.cpu.percentCPUUsage
      memKbTotal += m.memory.workingSetSize
    }

    const payload: PerfHudMetrics = {
      cpu: Math.round(cpuTotal),
      memMb: Math.round(memKbTotal / 1024),
    }

    // Skip IPC entirely if rounded values are unchanged: avoids DOM work and
    // keeps the channel quiet when the app is idle.
    if (
      this.lastPayload &&
      this.lastPayload.cpu === payload.cpu &&
      this.lastPayload.memMb === payload.memMb
    ) {
      return
    }
    this.lastPayload = payload

    for (const wc of this.subscribers.values()) {
      if (!wc.isDestroyed()) wc.send(CHANNEL, payload)
    }
  }
}
