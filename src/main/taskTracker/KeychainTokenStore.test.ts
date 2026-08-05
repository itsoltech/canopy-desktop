import { describe, expect, it } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import { KeychainTokenStore } from './KeychainTokenStore'

function fakePreferences(initial: Record<string, string> = {}): PreferencesStore {
  const values = new Map(Object.entries(initial))
  return {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => values.set(key, value),
    delete: (key: string) => values.delete(key),
    keysWithPrefix: (prefix: string) => [...values.keys()].filter((key) => key.startsWith(prefix)),
  } as unknown as PreferencesStore
}

describe('KeychainTokenStore capability facade', () => {
  it('does not use a generic GitHub tracker token for GitHub Actions', () => {
    const preferences = fakePreferences({
      'taskTracker.token.github:https://github.com': JSON.stringify({ token: 'tracker-token' }),
    })
    const store = new KeychainTokenStore(preferences)

    expect(
      store.resolveCredentials(
        'github-actions',
        'https://github.com/itsoltech/canopy-desktop',
        'actions.read',
      ),
    ).toBeNull()
    expect(store.listCredentials()[0]).toMatchObject({
      provider: 'github',
      intendedUses: ['tracker'],
      capabilities: ['issues.read', 'issues.write'],
    })
    expect(preferences.keysWithPrefix('taskTracker.token.')).toEqual([])
  })

  it('requires the repository audience for GitHub Actions', () => {
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials(
      'github-actions',
      'https://github.com/itsoltech/canopy-desktop',
      'actions-token',
    )

    expect(
      store.resolveCredentials(
        'github-actions',
        'https://github.com/itsoltech/other',
        'actions.read',
      ),
    ).toBeNull()
  })

  it('forks a shared credential when one binding replaces its token', () => {
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('jira', 'https://itsol.atlassian.net', 'old-token', undefined, 'tracker:a')
    expect(store.getCredentials('jira', 'https://itsol.atlassian.net', 'tracker:b')?.token).toBe(
      'old-token',
    )

    store.setCredentials('jira', 'https://itsol.atlassian.net', 'new-token', undefined, 'tracker:a')

    expect(store.getCredentials('jira', 'https://itsol.atlassian.net', 'tracker:a')?.token).toBe(
      'new-token',
    )
    expect(store.getCredentials('jira', 'https://itsol.atlassian.net', 'tracker:b')?.token).toBe(
      'old-token',
    )
    expect(store.listCredentials()).toHaveLength(2)
  })

  it('removes an unreferenced credential when a binding moves to another audience', () => {
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('jira', 'https://old.atlassian.net', 'old-token', undefined, 'tracker:a')

    store.setCredentials('jira', 'https://new.atlassian.net', 'new-token', undefined, 'tracker:a')

    expect(store.listCredentials()).toHaveLength(1)
    expect(store.listCredentials()[0].baseUrl).toBe('https://new.atlassian.net')
  })

  it('migrates a stable tracker token to the configured tracker binding and deletes it', () => {
    const baseUrl = 'https://itsol.atlassian.net'
    const preferences = fakePreferences({
      'taskTracker.connections': JSON.stringify([
        {
          id: 'jira-main',
          provider: 'jira',
          name: 'Jira',
          baseUrl,
          projectKey: 'ABC',
          authPrefKey: 'legacy-secret',
        },
      ]),
      [`taskTracker.token.jira:${baseUrl}`]: JSON.stringify({ token: 'legacy-token' }),
    })
    const store = new KeychainTokenStore(preferences)

    expect(store.getCredentials('jira', baseUrl, 'tracker:jira-main')?.token).toBe('legacy-token')
    expect(store.deleteCredentials('jira', baseUrl, 'tracker:jira-main')).toEqual({
      removed: true,
      retainedBindings: [],
    })
    expect(store.listCredentials()).toEqual([])
  })

  it('removes the temporary shared migration binding after a tracker auto-binds', () => {
    const baseUrl = 'https://itsol.atlassian.net'
    const store = new KeychainTokenStore(
      fakePreferences({
        [`taskTracker.token.jira:${baseUrl}`]: JSON.stringify({ token: 'legacy-token' }),
      }),
    )

    expect(store.getCredentials('jira', baseUrl, 'tracker:jira-main')?.token).toBe('legacy-token')
    expect(store.listCredentials()[0].bindings).toEqual(['tracker:jira-main'])
    expect(store.deleteCredentials('jira', baseUrl, 'tracker:jira-main').removed).toBe(true)
  })

  it('deletes a credential after pruning its only orphaned tracker binding', () => {
    const baseUrl = 'https://itsol.atlassian.net'
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('jira', baseUrl, 'token', undefined, 'tracker:live')
    expect(store.getCredentials('jira', baseUrl, 'tracker:orphan')?.token).toBe('token')

    expect(
      store.deleteCredentials('jira', baseUrl, 'tracker:live', new Set(['tracker:live'])),
    ).toEqual({ removed: true, retainedBindings: [] })
    expect(store.listCredentials()).toEqual([])
  })
})
