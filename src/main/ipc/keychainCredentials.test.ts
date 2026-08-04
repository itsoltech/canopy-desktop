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
    })
  })

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
})
