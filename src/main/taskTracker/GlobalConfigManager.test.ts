import { describe, expect, it } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { KeychainTokenStore } from './KeychainTokenStore'
import { GlobalConfigManager } from './GlobalConfigManager'

function fakePreferences(initial: Record<string, string> = {}): PreferencesStore {
  const values = new Map(Object.entries(initial))
  // Only the key/value surface GlobalConfigManager uses.
  return {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => values.set(key, value),
    delete: (key: string) => values.delete(key),
  } as unknown as PreferencesStore
}

function storedConfig(baseUrl: string): string {
  return JSON.stringify({
    version: 1,
    trackers: [{ id: 'jira-team', provider: 'jira', baseUrl }],
    filters: { assignedToMe: true, statuses: [] },
  })
}

describe('GlobalConfigManager', () => {
  it('reloads a config written directly to preferences (settings import) after invalidate()', () => {
    const prefs = fakePreferences({
      'taskTracker.migratedToGlobalConfig': '1',
      'taskTracker.globalConfig': storedConfig('https://old.example'),
    })
    // Migration is already flagged done, so the token store is never touched.
    const manager = new GlobalConfigManager(prefs, {} as KeychainTokenStore)
    expect(manager.load()?.trackers[0].baseUrl).toBe('https://old.example')

    prefs.set('taskTracker.globalConfig', storedConfig('https://imported.example'))
    manager.invalidate()

    expect(manager.load()?.trackers[0].baseUrl).toBe('https://imported.example')
  })
})
