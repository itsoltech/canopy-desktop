import { describe, expect, it } from 'vitest'
import { teamCityTokenCreationUrl } from './teamCityToken'

describe('teamCityTokenCreationUrl', () => {
  it.each(['https://tc.example.com', 'https://tc.example.com/'])(
    'opens the access-token page for %s without a doubled slash',
    (baseUrl) => {
      expect(teamCityTokenCreationUrl(baseUrl)).toBe(
        'https://tc.example.com/profile.html?item=accessTokens',
      )
    },
  )
})
