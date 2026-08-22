import { describe, expect, it, vi } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import { CredentialRegistry } from './CredentialRegistry'

function fakePreferences(): PreferencesStore {
  const values = new Map<string, string>()
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

describe('CredentialRegistry', () => {
  it('keeps two GitHub credentials for one host and resolves the explicitly bound capability', () => {
    const registry = new CredentialRegistry(fakePreferences())
    const tracker = registry.save({
      service: 'github',
      authMethod: 'pat',
      audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
      intendedUses: ['tracker'],
      capabilities: ['issues.read', 'issues.write'],
      secret: 'issues-token',
    })
    const actions = registry.save({
      service: 'github',
      authMethod: 'pat',
      audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
      intendedUses: ['github-actions'],
      capabilities: ['actions.read', 'contents.read', 'actions.dispatch'],
      secret: 'actions-token',
    })
    registry.bind('ci:github-actions:github.com/itsoltech/canopy-desktop', actions.id)

    expect(registry.list()).toHaveLength(2)
    expect(
      registry
        .resolve({
          bindingKey: 'ci:github-actions:github.com/itsoltech/canopy-desktop',
          service: 'github',
          audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
          capability: 'actions.dispatch',
        })
        ._unsafeUnwrap(),
    ).toMatchObject({ id: actions.id, secret: 'actions-token' })
    expect(
      registry
        .resolve({
          bindingKey: `tracker:${tracker.id}`,
          service: 'github',
          audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
          capability: 'issues.read',
        })
        ._unsafeUnwrap(),
    ).toMatchObject({ id: tracker.id, secret: 'issues-token' })
  })

  it('does not return an Actions credential for tracker or Git transport capabilities', () => {
    const registry = new CredentialRegistry(fakePreferences())
    registry.save({
      service: 'github',
      authMethod: 'pat',
      audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
      intendedUses: ['github-actions'],
      capabilities: ['actions.read', 'contents.read', 'actions.dispatch'],
      secret: 'actions-token',
    })

    expect(
      registry
        .resolve({
          bindingKey: 'tracker:repo:github-default',
          service: 'github',
          audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
          capability: 'issues.read',
        })
        ._unsafeUnwrapErr(),
    ).toEqual({ _tag: 'CredentialNotFound' })
    expect(
      registry
        .resolve({
          bindingKey: 'git-transport:origin',
          service: 'github',
          audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
          capability: 'git.push',
        })
        ._unsafeUnwrapErr(),
    ).toEqual({ _tag: 'CredentialNotFound' })
  })

  it('records a denied capability without invalidating other verified capabilities', () => {
    const registry = new CredentialRegistry(fakePreferences())
    const credential = registry.save({
      service: 'github',
      authMethod: 'pat',
      audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
      intendedUses: ['github-actions'],
      capabilities: ['actions.read', 'actions.dispatch'],
      secret: 'token',
    })

    registry.recordCapability(credential.id, 'actions.read', 'verified')
    registry.recordCapability(credential.id, 'actions.dispatch', 'denied', 'HTTP 403')

    const stored = registry.list().find((entry) => entry.id === credential.id)
    expect(stored?.verification['actions.read']?.state).toBe('verified')
    expect(stored?.verification['actions.dispatch']?.state).toBe('denied')
    expect(stored?.authenticationState).not.toBe('invalid')
  })

  it('redacts the stored secret from persisted verification reasons', () => {
    const registry = new CredentialRegistry(fakePreferences())
    const secret = 'tracker-secret-token'
    const credential = registry.save({
      service: 'jira',
      authMethod: 'api-token',
      audience: { host: 'itsol.atlassian.net' },
      intendedUses: ['tracker'],
      capabilities: ['issues.read'],
      secret,
    })

    registry.recordCapability(
      credential.id,
      'issues.read',
      'denied',
      `gateway echoed Authorization: Bearer ${secret}`,
    )

    expect(registry.list()[0].verification['issues.read']?.reason).toBe(
      'gateway echoed Authorization: Bearer [redacted]',
    )
  })

  it('redacts the secret the REQUEST used, not just the one stored when it lands', () => {
    // `setCredentials` reuses the credential id for a single-binding credential,
    // so a rotation while a request is in flight leaves the store holding a
    // different secret than the failing response echoed back.
    const preferences = fakePreferences()
    const registry = new CredentialRegistry(preferences)
    const usedSecret = 'old-token-in-flight'
    const credential = registry.save({
      service: 'jira',
      authMethod: 'api-token',
      audience: { host: 'itsol.atlassian.net' },
      intendedUses: ['tracker'],
      capabilities: ['issues.read'],
      secret: usedSecret,
    })

    // Rotation lands first: same id, new secret.
    registry.save({
      id: credential.id,
      service: 'jira',
      authMethod: 'api-token',
      audience: { host: 'itsol.atlassian.net' },
      intendedUses: ['tracker'],
      capabilities: ['issues.read'],
      secret: 'rotated-token',
    })

    registry.recordCapability(
      credential.id,
      'issues.read',
      'denied',
      `gateway echoed Authorization: Bearer ${usedSecret}`,
      usedSecret,
    )

    const reason = registry.list()[0].verification['issues.read']?.reason
    expect(reason).toBe('gateway echoed Authorization: Bearer [redacted]')
    expect(reason).not.toContain(usedSecret)
  })

  it('keeps a bound credential resolvable so stale 401 and 403 results can self-heal', () => {
    const registry = new CredentialRegistry(fakePreferences())
    const credential = registry.save({
      service: 'github',
      authMethod: 'pat',
      audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
      intendedUses: ['github-actions'],
      capabilities: ['actions.dispatch'],
      secret: 'token',
    })
    const bindingKey = 'ci:github-actions:github.com/itsoltech/canopy-desktop'
    registry.bind(bindingKey, credential.id)
    registry.recordAuthentication(credential.id, 'invalid')
    registry.recordCapability(credential.id, 'actions.dispatch', 'denied', 'HTTP 403')

    expect(
      registry
        .resolve({
          bindingKey,
          service: 'github',
          audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
          capability: 'actions.dispatch',
        })
        ._unsafeUnwrap(),
    ).toMatchObject({ id: credential.id, secret: 'token' })

    registry.recordAuthentication(credential.id, 'valid')
    registry.recordCapability(credential.id, 'actions.dispatch', 'verified')
    expect(registry.list()[0]).toMatchObject({
      authenticationState: 'valid',
      verification: { 'actions.dispatch': { state: 'verified' } },
    })
  })

  it('timestamps the authentication verdict, so the UI can say WHEN a token stopped working', () => {
    // A 401 records only the authentication state — per-capability verification keeps the
    // last verdict it earned, which can be a "verified" from days ago. Without a timestamp
    // of its own there is nothing to answer "since when is this token rejected".
    const registry = new CredentialRegistry(fakePreferences())
    const credential = registry.save({
      service: 'teamcity',
      authMethod: 'pat',
      audience: { host: 'tc.example.com', baseUrl: 'https://tc.example.com' },
      intendedUses: ['teamcity'],
      capabilities: ['builds.read'],
      secret: 'token',
    })
    registry.recordCapability(credential.id, 'builds.read', 'verified')

    // Pinned clock: both writes would otherwise land in the same millisecond and the
    // "moves forward" assertion below would pass or fail on timing alone.
    vi.useFakeTimers()
    try {
      vi.setSystemTime(new Date('2026-08-06T10:00:00.000Z'))
      registry.recordAuthentication(credential.id, 'invalid')
      const rejected = registry.list()[0]
      expect(rejected?.authenticationState).toBe('invalid')
      expect(rejected?.authenticationCheckedAt).toBe('2026-08-06T10:00:00.000Z')
      // The stale positive this exists for: the capability still claims it was verified.
      expect(rejected?.verification['builds.read']?.state).toBe('verified')

      // A later success must move it forward, or a banner would quote a stale moment.
      vi.setSystemTime(new Date('2026-08-06T11:30:00.000Z'))
      registry.recordSuccess(credential.id, 'builds.read')
      const recovered = registry.list()[0]
      expect(recovered?.authenticationState).toBe('valid')
      expect(recovered?.authenticationCheckedAt).toBe('2026-08-06T11:30:00.000Z')
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not delete a credential while bindings still depend on it', () => {
    const registry = new CredentialRegistry(fakePreferences())
    const credential = registry.save({
      service: 'jira',
      authMethod: 'api-token',
      audience: { host: 'itsol.atlassian.net' },
      intendedUses: ['tracker'],
      capabilities: ['issues.read', 'issues.write'],
      secret: 'token',
    })
    registry.bind('tracker:gakko:jira-default', credential.id)

    expect(registry.remove(credential.id)).toEqual({
      removed: false,
      bindings: ['tracker:gakko:jira-default'],
    })
    expect(registry.list()).toHaveLength(1)
  })

  it('rolls back a new secret when persisting its descriptor fails', () => {
    const preferences = fakePreferences()
    const originalSet = preferences.set.bind(preferences)
    vi.spyOn(preferences, 'set').mockImplementation((key, value) => {
      if (key === 'credential.registry.v2') throw new Error('registry write failed')
      originalSet(key, value)
    })
    const registry = new CredentialRegistry(preferences)

    expect(() =>
      registry.save({
        service: 'github',
        authMethod: 'pat',
        audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
        intendedUses: ['github-actions'],
        capabilities: ['actions.read'],
        secret: 'orphan-candidate',
      }),
    ).toThrow('registry write failed')
    expect(preferences.keysWithPrefix('credential.secret.v2.')).toEqual([])
  })

  it('rolls back a secret deletion when removing its descriptor fails', () => {
    const preferences = fakePreferences()
    const registry = new CredentialRegistry(preferences)
    const credential = registry.save({
      service: 'github',
      authMethod: 'pat',
      audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
      intendedUses: ['github-actions'],
      capabilities: ['actions.read'],
      secret: 'still-present',
    })
    const originalSet = preferences.set.bind(preferences)
    vi.spyOn(preferences, 'set').mockImplementation((key, value) => {
      if (key === 'credential.registry.v2') throw new Error('registry write failed')
      originalSet(key, value)
    })

    expect(() => registry.remove(credential.id)).toThrow('registry write failed')
    expect(registry.list()).toHaveLength(1)
    expect(preferences.keysWithPrefix('credential.secret.v2.')).toHaveLength(1)
  })

  it('distinguishes missing and ambiguous compatible credentials', () => {
    const registry = new CredentialRegistry(fakePreferences())
    const request = {
      bindingKey: 'tracker:jira-main',
      service: 'jira' as const,
      audience: { host: 'itsol.atlassian.net' },
      capability: 'issues.read' as const,
    }

    expect(registry.resolve(request)._unsafeUnwrapErr()).toEqual({ _tag: 'CredentialNotFound' })
    for (const secret of ['first', 'second']) {
      registry.save({
        service: 'jira',
        authMethod: 'api-token',
        audience: request.audience,
        intendedUses: ['tracker'],
        capabilities: ['issues.read'],
        secret,
      })
    }

    expect(registry.resolve(request)._unsafeUnwrapErr()).toEqual({
      _tag: 'CredentialAmbiguous',
      candidateCount: 2,
    })
  })

  it('records a verified success in one write and skips unchanged poll results', () => {
    const preferences = fakePreferences()
    const set = vi.spyOn(preferences, 'set')
    const registry = new CredentialRegistry(preferences)
    const credential = registry.save({
      service: 'teamcity',
      authMethod: 'access-token',
      audience: { host: 'tc.example.com' },
      intendedUses: ['teamcity'],
      capabilities: ['builds.read'],
      secret: 'token',
    })
    set.mockClear()

    registry.recordSuccess(credential.id, 'builds.read')
    expect(set).toHaveBeenCalledTimes(1)
    set.mockClear()

    registry.recordSuccess(credential.id, 'builds.read')
    expect(set).not.toHaveBeenCalled()
  })
})
