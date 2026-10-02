import { describe, expect, it } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import { TaskTrackerManager } from './TaskTrackerManager'

function fakePreferences(initial: Record<string, string>): {
  store: PreferencesStore
  values: Map<string, string>
} {
  const values = new Map(Object.entries(initial))
  // Minimal double: the legacy connection methods only use get/set/delete.
  const store = {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => {
      values.set(key, value)
    },
    delete: (key: string) => {
      values.delete(key)
    },
  } as unknown as PreferencesStore
  return { store, values }
}

const legacyConnection = {
  id: 'legacy-1',
  provider: 'jira',
  name: 'Jira',
  baseUrl: 'https://jira.example.com',
  projectKey: 'ABC',
  authPrefKey: 'taskTracker.token.legacy-1',
}

describe('TaskTrackerManager legacy connections', () => {
  it('never sends a preference outside the tracker token namespace as a token', async () => {
    const { store } = fakePreferences({
      'taskTracker.connections': JSON.stringify([
        { ...legacyConnection, authPrefKey: 'claude.apiKey' },
      ]),
      'claude.apiKey': 'sk-secret',
    })

    const result = await new TaskTrackerManager(store).testConnection('legacy-1')

    expect(result._unsafeUnwrapErr()._tag).toBe('AuthTokenMissing')
  })

  it('keeps the token key pinned when an update tries to repoint it', () => {
    const { store, values } = fakePreferences({
      'taskTracker.connections': JSON.stringify([legacyConnection]),
      'taskTracker.token.legacy-1': 'tracker-token',
    })
    const manager = new TaskTrackerManager(store)
    // IPC payloads are not bound by the TypeScript signature.
    const updates = { authPrefKey: 'remote.trustedDevices' } as unknown as Parameters<
      TaskTrackerManager['updateConnection']
    >[1]

    manager.updateConnection('legacy-1', updates, 'new-token')

    expect(manager.getConnections()[0].authPrefKey).toBe('taskTracker.token.legacy-1')
    expect(values.get('taskTracker.token.legacy-1')).toBe('new-token')
    expect(values.has('remote.trustedDevices')).toBe(false)
  })

  it('drops the stored token when the base URL moves to another host', () => {
    const { store, values } = fakePreferences({
      'taskTracker.connections': JSON.stringify([legacyConnection]),
      'taskTracker.token.legacy-1': 'tracker-token',
    })

    new TaskTrackerManager(store).updateConnection('legacy-1', {
      baseUrl: 'https://elsewhere.example.com',
    })

    expect(values.has('taskTracker.token.legacy-1')).toBe(false)
  })
})
