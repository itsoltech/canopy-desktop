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
