import { describe, expect, it } from 'vitest'
import { legacyConnectionTokenKey } from './legacyConnectionToken'

describe('legacyConnectionTokenKey', () => {
  it('returns the token key owned by the connection', () => {
    expect(legacyConnectionTokenKey({ id: 'abc', authPrefKey: 'taskTracker.token.abc' })).toBe(
      'taskTracker.token.abc',
    )
  })

  it('rejects a key that points at another stored secret', () => {
    expect(legacyConnectionTokenKey({ id: 'abc', authPrefKey: 'claude.apiKey' })).toBeNull()
    expect(
      legacyConnectionTokenKey({ id: 'abc', authPrefKey: 'taskTracker.token.other' }),
    ).toBeNull()
    expect(legacyConnectionTokenKey({ id: 'abc', authPrefKey: '' })).toBeNull()
  })
})
