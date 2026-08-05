import { describe, expect, it } from 'vitest'
import {
  normalizeCredentialToken,
  normalizedCredentialToken,
  normalizedTeamCityToken,
} from './token'

describe('credential token normalization', () => {
  it('trims a bounded token for every credential provider', () => {
    expect(normalizedCredentialToken('  token\r\n')).toBe('token')
    expect(normalizeCredentialToken('\t token \n')).toBe('token')
  })

  it.each([[' \r\n '], [' '.repeat(10_001)], [42]])(
    'rejects an unusable credential token',
    (value) => {
      expect(normalizedCredentialToken(value)).toBeNull()
      expect(() => normalizeCredentialToken(value)).toThrow(/credential token/i)
    },
  )
})

describe('normalizedTeamCityToken', () => {
  it('normalizes a bounded TeamCity token', () => {
    expect(normalizedTeamCityToken('  token\r\n')).toBe('token')
  })

  it.each([[' \r\n '], [' '.repeat(10_001)], [42]])(
    'returns null for an unusable value',
    (value) => {
      expect(normalizedTeamCityToken(value)).toBeNull()
    },
  )
})
