import { app, screen, session } from 'electron'
import { net } from 'electron'
import type { PreferencesStore } from '../db/PreferencesStore'

const UMAMI_URL = 'https://analytics.itsol.tech/api/send'
const WEBSITE_ID = 'e2ef58e3-bbc0-490c-9afb-263cbfce1640'

// Minimum wait between retries after a failed ping. Without this, every
// window-focus event while the endpoint is unreachable triggers a fresh POST,
// which pile up if the user is cycling focus between apps.
const PING_RETRY_BACKOFF_MS = 15 * 60 * 1000

export class TelemetryManager {
  private cachedPingDate: string | null = null
  private lastFailureAt = 0

  constructor(private readonly prefs: PreferencesStore) {}

  init(): void {
    this.cachedPingDate = this.prefs.get('telemetry.lastPingDate') ?? null
    this.tryPing()
  }

  onWindowFocus(): void {
    this.tryPing()
  }

  private tryPing(): void {
    if (this.prefs.get('telemetry.enabled') === 'false') return

    const today = new Date().toISOString().slice(0, 10)
    if (this.cachedPingDate === today) return
    if (this.lastFailureAt && Date.now() - this.lastFailureAt < PING_RETRY_BACKOFF_MS) return

    this.sendPing(today)
  }

  private sendPing(today: string): void {
    this.cachedPingDate = today
    this.prefs.set('telemetry.lastPingDate', today)

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

    const handleFailure = (): void => {
      this.cachedPingDate = null
      this.lastFailureAt = Date.now()
      this.prefs.delete('telemetry.lastPingDate')
    }

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
        if (!res.ok) {
          handleFailure()
        } else {
          this.lastFailureAt = 0
        }
      })
      .catch(handleFailure)
  }
}
