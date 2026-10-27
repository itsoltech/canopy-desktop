import { afterEach, describe, expect, it, vi } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import { TaskTrackerManager } from './TaskTrackerManager'
import type { TaskTrackerConnection } from './types'

function fakePreferences(initial: Record<string, string>): PreferencesStore {
  const values = new Map(Object.entries(initial))
  return {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => values.set(key, value),
    delete: (key: string) => values.delete(key),
  } as unknown as PreferencesStore
}

const connection: TaskTrackerConnection = {
  id: 'c1',
  provider: 'jira',
  name: 'Jira',
  baseUrl: 'https://jira.example.com',
  projectKey: 'ABC',
  authPrefKey: 'taskTracker.token.c1',
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('TaskTrackerManager legacy connection tokens', () => {
  it('never resolves another stored secret through a forged authPrefKey', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () => new Response('{}'))
    vi.stubGlobal('fetch', fetchMock)
    const manager = new TaskTrackerManager(
      fakePreferences({
        'taskTracker.connections': JSON.stringify([
          { ...connection, baseUrl: 'https://attacker.example', authPrefKey: 'claude.apiKey' },
        ]),
        'claude.apiKey': 'sk-secret',
      }),
    )

    const result = await manager.testConnection('c1')

    expect(result.isErr() && result.error._tag).toBe('AuthTokenMissing')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('keeps a connection on its own token when an update smuggles another key', async () => {
    const fetchMock = vi.fn<typeof fetch>(async () => new Response('{}'))
    vi.stubGlobal('fetch', fetchMock)
    const manager = new TaskTrackerManager(
      fakePreferences({
        'taskTracker.connections': JSON.stringify([connection]),
        'taskTracker.token.c1': 'tracker-token',
        'claude.apiKey': 'sk-secret',
      }),
    )

    // The IPC payload is untyped at runtime, so extra fields reach updateConnection.
    const smuggled = { authPrefKey: 'claude.apiKey' } as Partial<TaskTrackerConnection>
    manager.updateConnection('c1', smuggled)
    const result = await manager.testConnection('c1')

    expect(result.isOk()).toBe(true)
    const headers = JSON.stringify(fetchMock.mock.calls[0]?.[1]?.headers)
    expect(headers).toContain('tracker-token')
    expect(headers).not.toContain('sk-secret')
  })
})
