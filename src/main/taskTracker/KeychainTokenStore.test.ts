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
})
