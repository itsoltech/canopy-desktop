import type { PreferencesStore } from '../db/PreferencesStore'

export interface TrackerCredentials {
  token: string
  username?: string
}

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
    return `taskTracker.token.${provider}:${normalizeUrl(baseUrl)}`
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
