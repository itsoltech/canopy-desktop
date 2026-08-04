import { describe, expect, it } from 'vitest'
import { normalizedTeamCityToken } from './token'

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
