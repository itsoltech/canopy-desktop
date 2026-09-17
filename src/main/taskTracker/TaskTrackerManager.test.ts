import { describe, expect, it } from 'vitest'
import { TaskTrackerManager } from './TaskTrackerManager'
import type { PreferencesStore } from '../db/PreferencesStore'

// TaskTrackerManager only ever reaches for get/set/delete on the store, so a Map-backed
// stand-in covers the whole surface it touches without pulling in better-sqlite3.
function fakePreferencesStore(): PreferencesStore & { entries: Map<string, string> } {
  const entries = new Map<string, string>()
  const store = {
    entries,
    get: (key: string): string | null => entries.get(key) ?? null,
    set: (key: string, value: string): void => void entries.set(key, value),
    delete: (key: string): void => void entries.delete(key),
  }
  // Only get/set/delete are exercised by the code under test; the rest of the
  // PreferencesStore surface (encryption, migrations) is irrelevant here.
  return store as unknown as PreferencesStore & { entries: Map<string, string> }
}

function addJiraConnection(
  manager: TaskTrackerManager,
  baseUrl: string,
  token: string,
): { id: string; authPrefKey: string } {
  const conn = manager.addConnection(
    {
      provider: 'jira',
      name: 'Jira',
      baseUrl,
      projectKey: 'ENG',
      username: 'user@example.com',
    },
    token,
  )
  return { id: conn.id, authPrefKey: conn.authPrefKey }
}

describe('TaskTrackerManager.updateConnection', () => {
  // The renderer is the untrusted boundary: it can call taskTracker:updateConnection with an
  // arbitrary baseUrl. The stored token is resolved by authPrefKey (derived from the connection
  // id, not the host), so carrying it across an origin change would send a live Jira/YouTrack/
  // GitHub secret to whatever host the caller names on the next request.
  it('drops the stored token when baseUrl moves to a different origin without a new token', () => {
    const prefs = fakePreferencesStore()
    const manager = new TaskTrackerManager(prefs)
    const { id, authPrefKey } = addJiraConnection(manager, 'https://jira.example.com', 'secret-pat')

    manager.updateConnection(id, { baseUrl: 'https://attacker.example.com' })

    expect(prefs.get(authPrefKey)).toBeNull()
  })

  it('drops the stored token when an implicit (empty) baseUrl is repointed at a new origin', () => {
    const prefs = fakePreferencesStore()
    const manager = new TaskTrackerManager(prefs)
    const { id, authPrefKey } = addJiraConnection(manager, '', 'secret-pat')

    manager.updateConnection(id, { baseUrl: 'https://attacker.example.com' })

    expect(prefs.get(authPrefKey)).toBeNull()
  })

  // Guards against over-correcting: editing an unrelated field must not log the user out.
  it('keeps the stored token when baseUrl is resubmitted unchanged', () => {
    const prefs = fakePreferencesStore()
    const manager = new TaskTrackerManager(prefs)
    const { id, authPrefKey } = addJiraConnection(manager, 'https://jira.example.com', 'secret-pat')

    manager.updateConnection(id, { name: 'Renamed', baseUrl: 'https://jira.example.com' })

    expect(prefs.get(authPrefKey)).toBe('secret-pat')
  })

  it('keeps the stored token when only the path changes within the same origin', () => {
    const prefs = fakePreferencesStore()
    const manager = new TaskTrackerManager(prefs)
    const { id, authPrefKey } = addJiraConnection(manager, 'https://jira.example.com', 'secret-pat')

    manager.updateConnection(id, { baseUrl: 'https://jira.example.com/jira' })

    expect(prefs.get(authPrefKey)).toBe('secret-pat')
  })

  it('keeps the stored token when no baseUrl update is supplied at all', () => {
    const prefs = fakePreferencesStore()
    const manager = new TaskTrackerManager(prefs)
    const { id, authPrefKey } = addJiraConnection(manager, 'https://jira.example.com', 'secret-pat')

    manager.updateConnection(id, { projectKey: 'OPS' })

    expect(prefs.get(authPrefKey)).toBe('secret-pat')
  })

  it('stores the replacement token when one accompanies an origin change', () => {
    const prefs = fakePreferencesStore()
    const manager = new TaskTrackerManager(prefs)
    const { id, authPrefKey } = addJiraConnection(manager, 'https://jira.example.com', 'secret-pat')

    manager.updateConnection(id, { baseUrl: 'https://jira.other.com' }, 'fresh-pat')

    expect(prefs.get(authPrefKey)).toBe('fresh-pat')
  })
})
