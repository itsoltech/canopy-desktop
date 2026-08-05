import { describe, expect, it } from 'vitest'
import { normalizeKeychainCredentialPayload } from './keychainCredentials'

describe('normalizeKeychainCredentialPayload', () => {
  it('normalizes TeamCity tokens before persistence', () => {
    expect(
      normalizeKeychainCredentialPayload({
        provider: 'teamcity',
        baseUrl: 'https://tc.example.com',
        token: '  token\r\n',
      }),
    ).toEqual({
      provider: 'teamcity',
      baseUrl: 'https://tc.example.com',
      token: 'token',
      username: undefined,
      bindingKey: undefined,
    })
  })

  it.each(['jira', 'youtrack', 'github'])('normalizes %s tokens before persistence', (provider) => {
    expect(
      normalizeKeychainCredentialPayload({
        provider,
        baseUrl: 'https://service.example.com',
        token: '  token\r\n',
      }).token,
    ).toBe('token')
  })

  it.each(['', ' \r\n\t'])(
    'rejects an empty or whitespace-only token before persistence',
    (token) => {
      expect(() =>
        normalizeKeychainCredentialPayload({
          provider: 'jira',
          baseUrl: 'https://jira.example.com',
          token,
        }),
      ).toThrow('Credential token is required')
    },
  )

  it.each([[['teamcity']], [new String('teamcity')]])(
    'rejects a non-primitive provider before it can bypass TeamCity normalization',
    (provider) => {
      expect(() =>
        normalizeKeychainCredentialPayload({
          provider,
          baseUrl: 'https://tc.example.com',
          token: ' untrimmed ',
        }),
      ).toThrow('Provider and baseUrl are required')
    },
  )

  it('rejects a renderer-supplied CI binding and cross-purpose tracker binding', () => {
    expect(() =>
      normalizeKeychainCredentialPayload({
        provider: 'github-actions',
        baseUrl: 'https://github.com/itsoltech/canopy-desktop',
        token: 'token',
        bindingKey: 'tracker:jira-default',
      }),
    ).toThrow('does not match the provider purpose')
  })
})
