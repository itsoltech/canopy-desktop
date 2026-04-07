import { app, screen, session } from 'electron'
import { net } from 'electron'
import type { PreferencesStore } from '../db/PreferencesStore'

const UMAMI_URL = 'https://analytics.itsol.tech/api/send'
const WEBSITE_ID = 'e2ef58e3-bbc0-490c-9afb-263cbfce1640'

export class TelemetryManager {
  private lastPingDate: string | null = null

  constructor(private readonly prefs: PreferencesStore) {}

  init(): void {
    this.tryPing()
  }

  onWindowFocus(): void {
    this.tryPing()
  }

  private tryPing(): void {
    if (this.prefs.get('telemetry.enabled') === 'false') return

    const today = new Date().toISOString().slice(0, 10)
    if (this.lastPingDate === today) return

    this.sendPing(today)
  }

  private sendPing(today: string): void {
    const display = screen.getPrimaryDisplay()
    const { width, height } = display.size

    const body = JSON.stringify({
      type: 'event',
      payload: {
        website: WEBSITE_ID,
        hostname: 'app.canopy.itsol.tech',
        url: '/ping',
        language: app.getLocale(),
        screen: `${width}x${height}`,
        data: {
          version: app.getVersion(),
          os: process.platform,
          arch: process.arch,
        },
      },
    })

    net
      .fetch(UMAMI_URL, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'User-Agent': session.defaultSession.getUserAgent(),
        },
        body,
      })
      .then((res) => {
        if (res.ok) this.lastPingDate = today
      })
      .catch(() => {
        // silently ignore — next focus will retry
      })
  }
}
