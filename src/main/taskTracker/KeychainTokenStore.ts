import type { PreferencesStore } from '../db/PreferencesStore'

export interface TrackerCredentials {
  token: string
  username?: string
}

/** A stored credential entry WITHOUT the token — safe to cross IPC for the Settings list. */
export interface StoredTrackerCredential {
  provider: string
  baseUrl: string
  username?: string
}

const TOKEN_KEY_PREFIX = 'taskTracker.token.'

function normalizeUrl(baseUrl: string): string {
  try {
    const url = new URL(baseUrl)
    return (url.origin + url.pathname).replace(/\/$/, '')
  } catch {
    return baseUrl.replace(/\/$/, '')
  }
}

export class KeychainTokenStore {
  constructor(private preferencesStore: PreferencesStore) {}

  private buildKey(provider: string, baseUrl: string): string {
    return `${TOKEN_KEY_PREFIX}${provider}:${normalizeUrl(baseUrl)}`
  }

  /** Every credential stored on this machine, tokens omitted (key format: `provider:baseUrl`). */
  listCredentials(): StoredTrackerCredential[] {
    return this.preferencesStore
      .keysWithPrefix(TOKEN_KEY_PREFIX)
      .map((key): StoredTrackerCredential | null => {
        const id = key.slice(TOKEN_KEY_PREFIX.length)
        const sep = id.indexOf(':')
        if (sep <= 0) return null
        const provider = id.slice(0, sep)
        const baseUrl = id.slice(sep + 1)
        const creds = this.getCredentials(provider, baseUrl)
        return creds ? { provider, baseUrl, username: creds.username } : null
      })
      .filter((c): c is StoredTrackerCredential => c !== null)
  }

  getCredentials(provider: string, baseUrl: string): TrackerCredentials | null {
    const raw = this.preferencesStore.get(this.buildKey(provider, baseUrl))
    if (!raw) return null
    try {
      return JSON.parse(raw) as TrackerCredentials
    } catch {
      // Migration: old format stored plain token string
      return { token: raw }
    }
  }

  setCredentials(provider: string, baseUrl: string, token: string, username?: string): void {
    const creds: TrackerCredentials = { token, username }
    this.preferencesStore.set(this.buildKey(provider, baseUrl), JSON.stringify(creds))
  }

  deleteCredentials(provider: string, baseUrl: string): void {
    this.preferencesStore.delete(this.buildKey(provider, baseUrl))
  }

  hasCredentials(provider: string, baseUrl: string): boolean {
    return this.getCredentials(provider, baseUrl) !== null
  }
}
