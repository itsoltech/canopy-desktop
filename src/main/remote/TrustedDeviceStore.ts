import type { PreferencesStore } from '../db/PreferencesStore'

const PREF_KEY = 'remote.trustedDevices'

export interface TrustedDevice {
  deviceId: string
  /** Public key JWK for Phase 11 challenge-response (initially null for MVP). */
  publicKeyJwk: unknown
  name: string
  addedAt: string
  lastSeen: string
}

/**
 * Thin persistence layer for trusted remote devices. Backed by the
 * `PreferencesStore` as a JSON string so it survives app restarts without
 * any new database tables.
 *
 * Phase 11 MVP stores device identity but doesn't implement the full
 * challenge-response handshake — it just checks whether a `deviceId` is
 * in the list and skips the accept modal if so. The Web Crypto sign/verify
 * flow is deferred to Phase 14 hardening.
 */
export class TrustedDeviceStore {
  constructor(private prefs: PreferencesStore) {}

  list(): TrustedDevice[] {
    const raw = this.prefs.get(PREF_KEY)
    if (!raw) return []
    try {
      const parsed = JSON.parse(raw)
      return Array.isArray(parsed) ? parsed : []
    } catch {
      return []
    }
  }

  get(deviceId: string): TrustedDevice | undefined {
    return this.list().find((d) => d.deviceId === deviceId)
  }

  isTrusted(deviceId: string): boolean {
    return this.list().some((d) => d.deviceId === deviceId)
  }

  add(entry: TrustedDevice): void {
    const devices = this.list().filter((d) => d.deviceId !== entry.deviceId)
    devices.push(entry)
    this.prefs.set(PREF_KEY, JSON.stringify(devices))
  }

  updateLastSeen(deviceId: string): void {
    const devices = this.list()
    const device = devices.find((d) => d.deviceId === deviceId)
    if (!device) return
    device.lastSeen = new Date().toISOString()
    this.prefs.set(PREF_KEY, JSON.stringify(devices))
  }

  remove(deviceId: string): void {
    const devices = this.list().filter((d) => d.deviceId !== deviceId)
    this.prefs.set(PREF_KEY, JSON.stringify(devices))
  }

  removeAll(): void {
    this.prefs.delete(PREF_KEY)
  }
}
