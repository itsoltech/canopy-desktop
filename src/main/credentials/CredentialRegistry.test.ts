import { describe, expect, it } from 'vitest'
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
      registry.resolve({
        bindingKey: 'ci:github-actions:github.com/itsoltech/canopy-desktop',
        service: 'github',
        audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
        capability: 'actions.dispatch',
      }),
    ).toMatchObject({ id: actions.id, secret: 'actions-token' })
    expect(
      registry.resolve({
        bindingKey: `tracker:${tracker.id}`,
        service: 'github',
        audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
        capability: 'issues.read',
      }),
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
      registry.resolve({
        bindingKey: 'tracker:repo:github-default',
        service: 'github',
        audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
        capability: 'issues.read',
      }),
    ).toBeNull()
    expect(
      registry.resolve({
        bindingKey: 'git-transport:origin',
        service: 'github',
        audience: { host: 'github.com', repository: 'itsoltech/canopy-desktop' },
        capability: 'git.push',
      }),
    ).toBeNull()
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
})
