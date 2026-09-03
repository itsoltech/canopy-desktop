import { describe, expect, it, vi } from 'vitest'
import type { PreferencesStore } from '../db/PreferencesStore'
import {
  TeamCityOriginTrust,
  teamCityConfigBindingKey,
  teamCityDiscoveryBindingKey,
} from './teamcityAccess'

function fakePreferences(): PreferencesStore {
  const values = new Map<string, string>()
  return {
    get: (key: string) => values.get(key) ?? null,
    set: (key: string, value: string) => values.set(key, value),
  } as unknown as PreferencesStore
}

describe('TeamCity credential scopes', () => {
  it('binds approval to the canonical repository, exact server URL and exact build ids', () => {
    const base = teamCityConfigBindingKey('C:\\repo', {
      provider: 'teamcity',
      baseUrl: 'https://tc.example.com/teamcity',
      buildTypes: [
        { id: 'Deploy', label: 'Deploy prod' },
        { id: 'Build', label: 'Build' },
      ],
    })

    expect(
      teamCityConfigBindingKey('C:/repo', {
        provider: 'teamcity',
        baseUrl: 'https://tc.example.com/teamcity',
        buildTypes: [
          { id: 'Build', label: 'Renamed label' },
          { id: 'Deploy', label: 'Deploy' },
        ],
      }),
    ).toBe(base)
    expect(
      teamCityConfigBindingKey('C:/other', {
        provider: 'teamcity',
        baseUrl: 'https://tc.example.com/teamcity',
        buildTypes: [
          { id: 'Build', label: 'Build' },
          { id: 'Deploy', label: 'Deploy' },
        ],
      }),
    ).not.toBe(base)
    expect(
      teamCityConfigBindingKey('C:/Repo', {
        provider: 'teamcity',
        baseUrl: 'https://tc.example.com/teamcity',
        buildTypes: [
          { id: 'Build', label: 'Build' },
          { id: 'Deploy', label: 'Deploy' },
        ],
      }),
    ).not.toBe(base)
    expect(
      teamCityConfigBindingKey('C:/repo', {
        provider: 'teamcity',
        baseUrl: 'https://tc.example.com/teamcity',
        buildTypes: [{ id: 'Build', label: 'Build' }],
      }),
    ).not.toBe(base)
  })

  it('uses a separate repository-scoped approval for job discovery', () => {
    expect(teamCityDiscoveryBindingKey('C:/repo', 'https://tc.example.com')).not.toBe(
      teamCityDiscoveryBindingKey('C:/other', 'https://tc.example.com'),
    )
  })

  it('does not let separator characters collapse distinct approval tuples', () => {
    expect(teamCityDiscoveryBindingKey('/repo\nhttps://one.test', 'https://two.test')).not.toBe(
      teamCityDiscoveryBindingKey('/repo', 'https://one.test\nhttps://two.test'),
    )
  })
})

describe('TeamCityOriginTrust', () => {
  it('requires explicit approval before a private origin is allowed', async () => {
    const classify = vi.fn(async () => 'private' as const)
    const trust = new TeamCityOriginTrust(fakePreferences(), classify)

    expect(await trust.requiresApproval('https://teamcity.internal')).toBe(true)
    const denied = await trust.ensureAllowed('https://teamcity.internal')
    expect(denied.isErr() && denied.error._tag).toBe('CiPrivateOriginApprovalRequired')

    trust.approve('https://teamcity.internal')

    expect(await trust.requiresApproval('https://teamcity.internal')).toBe(false)
    expect(await trust.ensureAllowed('https://teamcity.internal')).toMatchObject({
      value: { allowPrivate: true },
    })
  })

  it('does not carry approval to another path on the same private host', async () => {
    const trust = new TeamCityOriginTrust(fakePreferences(), async () => 'private')
    trust.approve('https://teamcity.internal/TeamCity')

    expect(await trust.requiresApproval('https://teamcity.internal/Other')).toBe(true)
  })

  it('does not require consent for a public origin', async () => {
    const trust = new TeamCityOriginTrust(fakePreferences(), async () => 'public')

    expect(await trust.requiresApproval('https://tc.example.com')).toBe(false)
    expect(await trust.ensureAllowed('https://tc.example.com')).toMatchObject({
      value: { allowPrivate: false },
    })
  })

  it.each(['unresolved', 'invalid'] as const)(
    'fails closed for a %s origin',
    async (networkClass) => {
      const trust = new TeamCityOriginTrust(fakePreferences(), async () => networkClass)

      await expect(trust.requiresApproval('https://tc.example.com')).rejects.toThrow()
      const result = await trust.ensureAllowed('https://tc.example.com')

      expect(result.isErr() && result.error._tag).toBe('CiApiError')
    },
  )
})
