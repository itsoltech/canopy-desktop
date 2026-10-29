import { afterEach, describe, expect, it, vi } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import { TaskTrackerManager } from './TaskTrackerManager'

const SECRET_KEY = 'claude.apiKey'
const SECRET = 'sk-ant-secret'

function fakePreferences(initial: Record<string, string> = {}): PreferencesStore {
  const values = new Map(Object.entries(initial))
  // Only the members TaskTrackerManager touches for legacy connections are faked.
  return {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => values.set(key, value),
    delete: (key: string) => values.delete(key),
  } as unknown as PreferencesStore
}

describe('TaskTrackerManager legacy connection tokens', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('keeps the connection id and token key when an update tries to rewrite them', () => {
    const preferences = fakePreferences({ [SECRET_KEY]: SECRET })
    const manager = new TaskTrackerManager(preferences)
    const connection = manager.addConnection(
      { provider: 'jira', name: 'Jira', baseUrl: 'https://jira.example.com', projectKey: 'ABC' },
      'tracker-token',
    )

    // Updates arrive over IPC, so the payload can carry fields its static type omits.
    const forged = { id: 'other', authPrefKey: SECRET_KEY } as unknown as Parameters<
      TaskTrackerManager['updateConnection']
    >[1]
    manager.updateConnection(connection.id, forged)
    manager.updateConnection(connection.id, {}, 'rotated-token')

    expect(manager.getConnections().map((c) => c.id)).toEqual([connection.id])
    expect(preferences.get(`taskTracker.token.${connection.id}`)).toBe('rotated-token')
    expect(preferences.get(SECRET_KEY)).toBe(SECRET)
  })

  it('only uses the token stored for the connection id, whatever key the record names', async () => {
    const preferences = fakePreferences({
      [SECRET_KEY]: SECRET,
      'taskTracker.token.c1': 'tracker-token',
      'taskTracker.connections': JSON.stringify([
        {
          id: 'c1',
          provider: 'jira',
          name: 'Jira',
          baseUrl: 'https://jira.example.com',
          projectKey: 'ABC',
          authPrefKey: SECRET_KEY,
        },
      ]),
    })
    const fetchMock = vi.fn<typeof fetch>(async () => new Response('{}'))
    vi.stubGlobal('fetch', fetchMock)
    const manager = new TaskTrackerManager(preferences)

    const result = await manager.testConnection('c1')

    expect(result.isOk()).toBe(true)
    expect(fetchMock.mock.calls[0]?.[1]?.headers).toMatchObject({
      Authorization: 'Bearer tracker-token',
    })

    manager.removeConnection('c1')

    expect(preferences.get(SECRET_KEY)).toBe(SECRET)
    expect(preferences.get('taskTracker.token.c1')).toBeNull()
  })
})
