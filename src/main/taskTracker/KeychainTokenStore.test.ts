import { describe, expect, it, vi } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import { KeychainTokenStore } from './KeychainTokenStore'

function fakePreferences(initial: Record<string, string> = {}): PreferencesStore {
  const values = new Map(Object.entries(initial))
  return {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => values.set(key, value),
    delete: (key: string) => values.delete(key),
    keysWithPrefix: (prefix: string) => [...values.keys()].filter((key) => key.startsWith(prefix)),
    runInTransaction: <T>(operation: () => T): T => {
      const snapshot = new Map(values)
      try {
        return operation()
      } catch (error) {
        values.clear()
        for (const [key, value] of snapshot) values.set(key, value)
        throw error
      }
    },
  } as unknown as PreferencesStore
}

describe('KeychainTokenStore capability facade', () => {
  it('does not auto-bind a server credential into a repository approval scope', () => {
    const store = new KeychainTokenStore(fakePreferences())
    const baseUrl = 'https://tc.example.com'
    store.setCredentials('teamcity', baseUrl, 'token')

    const scoped = store.resolveApprovedCredentialsResult(
      'teamcity',
      baseUrl,
      'builds.read',
      'ci:teamcity:repo:scope-a',
    )

    expect(scoped.isErr() && scoped.error._tag).toBe('CredentialApprovalRequired')
    expect(store.registry.listBindings()['ci:teamcity:repo:scope-a']).toBeUndefined()
  })

  it('uses a server credential only after it is explicitly bound to the repository scope', () => {
    const store = new KeychainTokenStore(fakePreferences())
    const baseUrl = 'https://tc.example.com'
    store.setCredentials('teamcity', baseUrl, 'token')
    const approval = store
      .prepareCredentialsBindingsApproval('teamcity', baseUrl, 'builds.read', [
        'ci:teamcity:repo:scope-a',
      ])
      ._unsafeUnwrap()

    expect(
      store
        .approveCredentialsBinding(
          'teamcity',
          baseUrl,
          'builds.read',
          'ci:teamcity:repo:scope-a',
          approval,
        )
        .isOk(),
    ).toBe(true)
    expect(
      store.resolveApprovedCredentialsResult(
        'teamcity',
        baseUrl,
        'builds.trigger',
        'ci:teamcity:repo:scope-a',
      ),
    ).toMatchObject({ value: expect.objectContaining({ token: 'token' }) })
  })

  it('requires renewed approval when the shared TeamCity credential changes identity', () => {
    const store = new KeychainTokenStore(fakePreferences())
    const baseUrl = 'https://tc.example.com'
    const bindingKey = 'ci:teamcity:repo-config:scope-a'
    const oldCredential = store.setCredentials('teamcity', baseUrl, 'old-token')._unsafeUnwrap()
    const approval = store
      .prepareCredentialsBindingsApproval('teamcity', baseUrl, 'builds.read', [bindingKey])
      ._unsafeUnwrap()
    store.approveCredentialsBinding('teamcity', baseUrl, 'builds.read', bindingKey, approval)
    store.setCredentials('teamcity', baseUrl, 'new-token')

    const scoped = store.resolveApprovedCredentialsResult(
      'teamcity',
      baseUrl,
      'builds.read',
      bindingKey,
    )

    expect(scoped.isErr() && scoped.error._tag).toBe('CredentialApprovalRequired')
    expect(store.registry.listBindings()[bindingKey]).toBeUndefined()
    expect(store.registry.list().some((credential) => credential.id === oldCredential.id)).toBe(
      false,
    )
  })

  it('does not let a late result from the old TeamCity token approve its replacement', () => {
    const store = new KeychainTokenStore(fakePreferences())
    const baseUrl = 'https://tc.example.com'
    const bindingKey = 'ci:teamcity:repo-config:scope-a'
    store.setCredentials('teamcity', baseUrl, 'old-token')
    const approval = store
      .prepareCredentialsBindingsApproval('teamcity', baseUrl, 'builds.read', [bindingKey])
      ._unsafeUnwrap()
    store.approveCredentialsBinding('teamcity', baseUrl, 'builds.read', bindingKey, approval)
    const inFlight = store
      .resolveApprovedCredentialsResult('teamcity', baseUrl, 'builds.read', bindingKey)
      ._unsafeUnwrap()

    store.setCredentials('teamcity', baseUrl, 'new-token')
    store.recordResult('teamcity', baseUrl, 'builds.read', 200, undefined, {
      bindingKey,
      usedSecret: inFlight.token,
    })

    expect(store.registry.listBindings()[bindingKey]).toBeUndefined()
    const resolved = store.resolveApprovedCredentialsResult(
      'teamcity',
      baseUrl,
      'builds.read',
      bindingKey,
    )
    expect(resolved.isErr() && resolved.error._tag).toBe('CredentialApprovalRequired')
  })

  it('removes repository approvals and the secret when an approved TeamCity credential is deleted', () => {
    const preferences = fakePreferences()
    const store = new KeychainTokenStore(preferences)
    const baseUrl = 'https://tc.example.com'
    const bindingKey = 'ci:teamcity:repo-config:scope-a'
    store.setCredentials('teamcity', baseUrl, 'token')
    const approval = store
      .prepareCredentialsBindingsApproval('teamcity', baseUrl, 'builds.read', [bindingKey])
      ._unsafeUnwrap()
    store.approveCredentialsBinding('teamcity', baseUrl, 'builds.read', bindingKey, approval)

    const removed = store.deleteCredentials('teamcity', baseUrl)

    expect(removed).toEqual({ removed: true, retainedBindings: [] })
    expect(store.registry.listBindings()[bindingKey]).toBeUndefined()
    expect(store.registry.list()).toEqual([])
    expect(store.listCredentials()).toEqual([])
    expect(preferences.keysWithPrefix('credential.secret.v2.')).toEqual([])
  })

  it('rolls back every repository scope when a multi-binding approval cannot be persisted', () => {
    const preferences = fakePreferences()
    const store = new KeychainTokenStore(preferences)
    const baseUrl = 'https://tc.example.com'
    store.setCredentials('teamcity', baseUrl, 'token')
    const approval = store
      .prepareCredentialsBindingsApproval('teamcity', baseUrl, 'builds.read', [
        'ci:teamcity:repo:config',
        'ci:teamcity:repo:discovery',
      ])
      ._unsafeUnwrap()
    const originalSet = preferences.set.bind(preferences)
    let bindingWrites = 0
    vi.spyOn(preferences, 'set').mockImplementation((key, value) => {
      if (key === 'credential.bindings.v2' && ++bindingWrites === 2) {
        throw new Error('second approval binding failed')
      }
      originalSet(key, value)
    })

    expect(() =>
      store.approveCredentialsBindings(
        'teamcity',
        baseUrl,
        'builds.read',
        ['ci:teamcity:repo:config', 'ci:teamcity:repo:discovery'],
        approval,
      ),
    ).toThrow('second approval binding failed')
    expect(store.registry.listBindings()['ci:teamcity:repo:config']).toBeUndefined()
    expect(store.registry.listBindings()['ci:teamcity:repo:discovery']).toBeUndefined()
  })

  it('rejects approval when a same-id credential secret changes during confirmation', () => {
    const store = new KeychainTokenStore(fakePreferences())
    const baseUrl = 'https://tc.example.com'
    const bindingKey = 'ci:teamcity:repo:scope-a'
    store.setCredentials('teamcity', baseUrl, 'old-token')
    const approval = store
      .prepareCredentialsBindingsApproval('teamcity', baseUrl, 'builds.read', [bindingKey])
      ._unsafeUnwrap()

    store.setCredentials('teamcity', baseUrl, 'new-token')
    const result = store.approveCredentialsBinding(
      'teamcity',
      baseUrl,
      'builds.read',
      bindingKey,
      approval,
    )

    expect(result.isErr() && result.error._tag).toBe('CredentialApprovalRequired')
    expect(store.registry.listBindings()[bindingKey]).toBeUndefined()
  })

  it('rejects approval when credential identity changes during confirmation', () => {
    const store = new KeychainTokenStore(fakePreferences())
    const baseUrl = 'https://tc.example.com'
    const bindingKey = 'ci:teamcity:repo:scope-a'
    store.setCredentials('teamcity', baseUrl, 'old-token')
    const approval = store
      .prepareCredentialsBindingsApproval('teamcity', baseUrl, 'builds.read', [bindingKey])
      ._unsafeUnwrap()
    store.registry.bind('test:shared-binding', approval.credentialId)

    store.setCredentials('teamcity', baseUrl, 'new-token')
    const result = store.approveCredentialsBinding(
      'teamcity',
      baseUrl,
      'builds.read',
      bindingKey,
      approval,
    )

    expect(result.isErr() && result.error._tag).toBe('CredentialApprovalRequired')
    expect(store.registry.listBindings()[bindingKey]).toBeUndefined()
  })

  it('does not reactivate an old approved TeamCity token after removing its replacement', () => {
    const store = new KeychainTokenStore(fakePreferences())
    const baseUrl = 'https://tc.example.com'
    const bindingKey = 'ci:teamcity:repo-config:scope-a'
    store.setCredentials('teamcity', baseUrl, 'old-token')
    const approval = store
      .prepareCredentialsBindingsApproval('teamcity', baseUrl, 'builds.read', [bindingKey])
      ._unsafeUnwrap()
    store.approveCredentialsBinding('teamcity', baseUrl, 'builds.read', bindingKey, approval)
    store.setCredentials('teamcity', baseUrl, 'new-token')

    store.deleteCredentials('teamcity', baseUrl)
    const result = store.resolveApprovedCredentialsResult(
      'teamcity',
      baseUrl,
      'builds.read',
      bindingKey,
    )

    expect(result.isErr() && result.error._tag).toBe('CredentialNotFound')
  })

  it('treats a probe-specific 403 as rejected authentication, not denied Actions access', () => {
    const baseUrl = 'https://github.com/itsoltech/canopy-desktop'
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('github-actions', baseUrl, 'token')

    store.recordResult('github-actions', baseUrl, 'actions.read', 403, 'Forbidden', {
      usedSecret: 'token',
      authenticationRejected: true,
    })

    expect(store.registry.list()[0]).toMatchObject({
      authenticationState: 'invalid',
      verification: {},
    })
  })

  it('keeps an ordinary 403 scoped to the denied capability', () => {
    const baseUrl = 'https://github.com/itsoltech/canopy-desktop'
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('github-actions', baseUrl, 'token')

    store.recordResult('github-actions', baseUrl, 'actions.read', 403, 'Forbidden', {
      usedSecret: 'token',
    })

    expect(store.registry.list()[0]).toMatchObject({
      authenticationState: 'unknown',
      verification: { 'actions.read': { state: 'denied', reason: 'Forbidden' } },
    })
  })

  it('ignores a rejected result produced by the token that was replaced in flight', () => {
    const baseUrl = 'https://github.com/itsoltech/canopy-desktop'
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('github-actions', baseUrl, 'old-token')
    store.setCredentials('github-actions', baseUrl, 'new-token')

    store.recordResult('github-actions', baseUrl, 'actions.read', 401, 'Bad credentials', {
      usedSecret: 'old-token',
    })

    expect(store.registry.list()[0]?.authenticationState).toBe('unknown')
  })

  it('ignores a successful result produced by the token that was replaced in flight', () => {
    const baseUrl = 'https://github.com/itsoltech/canopy-desktop'
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('github-actions', baseUrl, 'old-token')
    store.setCredentials('github-actions', baseUrl, 'new-token')

    store.recordResult('github-actions', baseUrl, 'actions.read', 200, undefined, {
      usedSecret: 'old-token',
    })

    expect(store.registry.list()[0]).toMatchObject({
      authenticationState: 'unknown',
      verification: {},
    })
  })

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

  it('rolls back the secret and descriptor when persisting its binding fails', () => {
    const preferences = fakePreferences()
    const persist = preferences.set.bind(preferences)
    vi.spyOn(preferences, 'set').mockImplementation((key, value) => {
      if (key === 'credential.bindings.v2') throw new Error('binding write failed')
      persist(key, value)
    })
    const store = new KeychainTokenStore(preferences)

    expect(() =>
      store.setCredentials(
        'github-actions',
        'https://github.com/itsoltech/canopy-desktop',
        'token',
      ),
    ).toThrow('binding write failed')

    expect(preferences.get('credential.registry.v2')).toBeNull()
    expect(preferences.keysWithPrefix('credential.secret.v2.')).toEqual([])
  })

  it('restores the binding and secret when deleting the descriptor fails', () => {
    const preferences = fakePreferences()
    const store = new KeychainTokenStore(preferences)
    const baseUrl = 'https://github.com/itsoltech/canopy-desktop'
    store.setCredentials('github-actions', baseUrl, 'token')
    const persist = preferences.set.bind(preferences)
    vi.spyOn(preferences, 'set').mockImplementation((key, value) => {
      if (key === 'credential.registry.v2') throw new Error('registry write failed')
      persist(key, value)
    })

    expect(() => store.deleteCredentials('github-actions', baseUrl)).toThrow(
      'registry write failed',
    )

    expect(store.getCredentials('github-actions', baseUrl)?.token).toBe('token')
    expect(store.listCredentials()[0]?.bindings).toEqual([
      'ci:github-actions:github.com/itsoltech/canopy-desktop',
    ])
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

  it('rolls back auto-binding and shared-binding cleanup when the second write fails', () => {
    const baseUrl = 'https://itsol.atlassian.net'
    const preferences = fakePreferences({
      [`taskTracker.token.jira:${baseUrl}`]: JSON.stringify({ token: 'legacy-token' }),
    })
    const store = new KeychainTokenStore(preferences)
    const originalBindings = store.listCredentials()[0].bindings
    const persist = preferences.set.bind(preferences)
    let bindingWrites = 0
    const setSpy = vi.spyOn(preferences, 'set').mockImplementation((key, value) => {
      if (key === 'credential.bindings.v2' && ++bindingWrites === 2) {
        throw new Error('cleanup write failed')
      }
      persist(key, value)
    })

    expect(() => store.getCredentials('jira', baseUrl, 'tracker:jira-main')).toThrow(
      'cleanup write failed',
    )
    expect(store.listCredentials()[0].bindings).toEqual(originalBindings)

    setSpy.mockRestore()
    expect(store.getCredentials('jira', baseUrl, 'tracker:jira-main')?.token).toBe('legacy-token')
    expect(store.listCredentials()[0].bindings).toEqual(['tracker:jira-main'])
  })

  it('rolls back the complete multi-binding stable-token migration and retries cleanly', () => {
    const baseUrl = 'https://itsol.atlassian.net'
    const legacyKey = `taskTracker.token.jira:${baseUrl}`
    const preferences = fakePreferences({
      'taskTracker.connections': JSON.stringify([
        { id: 'jira-a', provider: 'jira', name: 'A', baseUrl, projectKey: 'A' },
        { id: 'jira-b', provider: 'jira', name: 'B', baseUrl, projectKey: 'B' },
      ]),
      [legacyKey]: JSON.stringify({ token: 'legacy-token' }),
    })
    const persist = preferences.set.bind(preferences)
    let bindingWrites = 0
    const setSpy = vi.spyOn(preferences, 'set').mockImplementation((key, value) => {
      if (key === 'credential.bindings.v2' && ++bindingWrites === 2) {
        throw new Error('second binding write failed')
      }
      persist(key, value)
    })

    expect(() => new KeychainTokenStore(preferences)).toThrow('second binding write failed')
    expect(preferences.get('credential.registry.v2')).toBeNull()
    expect(preferences.keysWithPrefix('credential.secret.v2.')).toEqual([])
    expect(preferences.get(legacyKey)).not.toBeNull()

    setSpy.mockRestore()
    const retried = new KeychainTokenStore(preferences)
    expect(retried.listCredentials()[0].bindings).toEqual(['tracker:jira-a', 'tracker:jira-b'])
    expect(preferences.get(legacyKey)).toBeNull()
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

  it('retains a credential for a tracker proven live in another config', () => {
    const baseUrl = 'https://itsol.atlassian.net'
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('jira', baseUrl, 'token', undefined, 'tracker:current')
    expect(store.getCredentials('jira', baseUrl, 'tracker:other-repo')?.token).toBe('token')

    expect(
      store.deleteCredentials(
        'jira',
        baseUrl,
        'tracker:current',
        new Set(['tracker:current', 'tracker:other-repo']),
      ),
    ).toEqual({ removed: false, retainedBindings: ['tracker:other-repo'] })
  })

  it('never prunes a CI binding while removing orphaned tracker bindings', () => {
    const baseUrl = 'https://teamcity.example.com'
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('teamcity', baseUrl, 'token')
    const credential = store.listCredentials()[0]
    store.registry.bind('tracker:orphan', credential.id)

    expect(store.deleteCredentials('teamcity', baseUrl, 'tracker:orphan', new Set())).toEqual({
      removed: false,
      retainedBindings: [`ci:teamcity:${baseUrl}`],
    })
    expect(store.getCredentials('teamcity', baseUrl)?.token).toBe('token')
  })

  it('keeps TeamCity context-path casing distinct', () => {
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('teamcity', 'https://tc.example.com/teamcity', 'lower-token')
    store.setCredentials('teamcity', 'https://tc.example.com/TeamCity', 'mixed-token')

    expect(store.getCredentials('teamcity', 'https://tc.example.com/teamcity')?.token).toBe(
      'lower-token',
    )
    expect(store.getCredentials('teamcity', 'https://tc.example.com/TeamCity')?.token).toBe(
      'mixed-token',
    )
  })

  it('migrates the legacy lower-cased TeamCity binding when its audience matches', () => {
    const baseUrl = 'https://tc.example.com/TeamCity'
    const store = new KeychainTokenStore(fakePreferences())
    const credential = store.registry.save({
      service: 'teamcity',
      authMethod: 'pat',
      audience: { host: 'tc.example.com', baseUrl },
      intendedUses: ['teamcity'],
      capabilities: ['builds.read', 'builds.trigger'],
      secret: 'token',
    })
    store.registry.bind(`ci:teamcity:${baseUrl.toLowerCase()}`, credential.id)

    expect(store.getCredentials('teamcity', baseUrl)?.token).toBe('token')
    expect(store.listCredentials()[0].bindings).toEqual([`ci:teamcity:${baseUrl}`])
  })

  it('can delete a matching legacy TeamCity binding before it is otherwise resolved', () => {
    const baseUrl = 'https://tc.example.com/TeamCity'
    const store = new KeychainTokenStore(fakePreferences())
    const credential = store.registry.save({
      service: 'teamcity',
      authMethod: 'pat',
      audience: { host: 'tc.example.com', baseUrl },
      intendedUses: ['teamcity'],
      capabilities: ['builds.read', 'builds.trigger'],
      secret: 'token',
    })
    store.registry.bind(`ci:teamcity:${baseUrl.toLowerCase()}`, credential.id)

    expect(store.deleteCredentials('teamcity', baseUrl)).toEqual({
      removed: true,
      retainedBindings: [],
    })
    expect(store.listCredentials()).toEqual([])
  })

  it('cleans a duplicate legacy alias before deleting the current TeamCity binding', () => {
    const baseUrl = 'https://tc.example.com/TeamCity'
    const store = new KeychainTokenStore(fakePreferences())
    store.setCredentials('teamcity', baseUrl, 'token')
    const credential = store.listCredentials()[0]
    store.registry.bind(`ci:teamcity:${baseUrl.toLowerCase()}`, credential.id)

    expect(store.deleteCredentials('teamcity', baseUrl)).toEqual({
      removed: true,
      retainedBindings: [],
    })
    expect(store.listCredentials()).toEqual([])
  })
})
