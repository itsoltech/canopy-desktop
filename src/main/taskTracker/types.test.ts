import { describe, expect, it } from 'vitest'
import { legacyConnectionTokenKey } from './types'

describe('legacyConnectionTokenKey', () => {
  it('returns the canonical per-connection token key', () => {
    expect(legacyConnectionTokenKey({ id: 'abc', authPrefKey: 'taskTracker.token.abc' })).toBe(
      'taskTracker.token.abc',
    )
  })

  it.each(['claude.apiKey', 'credential.secret.v2.abc', 'taskTracker.token.other'])(
    'refuses a record whose authPrefKey points at %s',
    (authPrefKey) => {
      expect(legacyConnectionTokenKey({ id: 'abc', authPrefKey })).toBeNull()
    },
  )
})
