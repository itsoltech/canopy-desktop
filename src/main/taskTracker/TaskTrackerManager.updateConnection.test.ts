import { describe, expect, it } from 'vitest'
import { TaskTrackerManager } from './TaskTrackerManager'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { TaskTrackerConnection } from './types'

const CONNECTIONS_PREF_KEY = 'taskTracker.connections'

/** In-memory stand-in for the SQLite-backed store — keeps the test free of native modules. */
function fakePreferencesStore(seed: Record<string, string> = {}): PreferencesStore {
  const values = new Map(Object.entries(seed))
  return {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => void values.set(key, value),
    delete: (key: string) => void values.delete(key),
  } as unknown as PreferencesStore
}

const CONNECTION: TaskTrackerConnection = {
  id: 'conn-1',
  authPrefKey: 'taskTracker.token.conn-1',
  provider: 'jira',
  name: 'Work Jira',
  baseUrl: 'https://jira.example.com',
  projectKey: 'ENG',
}

function managerWithConnection(): {
  manager: TaskTrackerManager
  store: PreferencesStore
} {
  const store = fakePreferencesStore({
    [CONNECTIONS_PREF_KEY]: JSON.stringify([CONNECTION]),
    [CONNECTION.authPrefKey]: 'secret-token',
  })
  return { manager: new TaskTrackerManager(store), store }
}

describe('TaskTrackerManager.updateConnection', () => {
  it('refuses to repoint an existing connection at a new origin without a fresh token', () => {
    const { manager, store } = managerWithConnection()

    const result = manager.updateConnection('conn-1', { baseUrl: 'https://attacker.example' })

    expect(result.isErr()).toBe(true)
    if (result.isErr()) {
      expect(result.error._tag).toBe('BaseUrlChangeRequiresToken')
    }
    // The stored connection must be untouched, so the existing token is never
    // sent to the new origin by a later fetchTasks/testConnection call.
    const stored = JSON.parse(store.get(CONNECTIONS_PREF_KEY)!) as TaskTrackerConnection[]
    expect(stored[0].baseUrl).toBe('https://jira.example.com')
  })

  it('refuses a same-host origin change that only swaps the scheme or port', () => {
    const { manager } = managerWithConnection()

    expect(manager.updateConnection('conn-1', { baseUrl: 'http://jira.example.com' }).isErr()).toBe(
      true,
    )
    expect(
      manager.updateConnection('conn-1', { baseUrl: 'https://jira.example.com:8443' }).isErr(),
    ).toBe(true)
  })

  it('allows a base URL change when a fresh token is supplied', () => {
    const { manager, store } = managerWithConnection()

    const result = manager.updateConnection(
      'conn-1',
      { baseUrl: 'https://jira.example.net' },
      'new-token',
    )

    expect(result.isOk()).toBe(true)
    expect(store.get(CONNECTION.authPrefKey)).toBe('new-token')
    const stored = JSON.parse(store.get(CONNECTIONS_PREF_KEY)!) as TaskTrackerConnection[]
    expect(stored[0].baseUrl).toBe('https://jira.example.net')
  })

  it('allows non-baseUrl edits without a token', () => {
    const { manager, store } = managerWithConnection()

    const result = manager.updateConnection('conn-1', { name: 'Renamed' })

    expect(result.isOk()).toBe(true)
    const stored = JSON.parse(store.get(CONNECTIONS_PREF_KEY)!) as TaskTrackerConnection[]
    expect(stored[0].name).toBe('Renamed')
    expect(stored[0].baseUrl).toBe('https://jira.example.com')
    // Untouched token: an unchanged origin must not force re-authentication.
    expect(store.get(CONNECTION.authPrefKey)).toBe('secret-token')
  })

  it('treats an identical base URL as no change', () => {
    const { manager } = managerWithConnection()

    expect(manager.updateConnection('conn-1', { baseUrl: 'https://jira.example.com' }).isOk()).toBe(
      true,
    )
  })

  it('reports a missing connection as ConnectionNotFound', () => {
    const { manager } = managerWithConnection()

    const result = manager.updateConnection('nope', { name: 'x' })

    expect(result.isErr()).toBe(true)
    if (result.isErr()) {
      expect(result.error._tag).toBe('ConnectionNotFound')
    }
  })
})
