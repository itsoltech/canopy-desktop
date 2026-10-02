import { describe, expect, it } from 'vitest'
import { redactSecret } from './redactSecret'

describe('redactSecret', () => {
  it('replaces every occurrence of the secret', () => {
    expect(redactSecret('auth failed for ghp_abc (token ghp_abc)', 'ghp_abc')).toBe(
      'auth failed for [redacted] (token [redacted])',
    )
  })

  it('leaves the message untouched when the secret does not appear', () => {
    expect(redactSecret('403 Forbidden', 'ghp_abc')).toBe('403 Forbidden')
  })

  it('leaves the message untouched when no secret is supplied', () => {
    expect(redactSecret('403 Forbidden', undefined)).toBe('403 Forbidden')
  })

  // `replaceAll('')` splices the marker between every character, which would turn a
  // clean error message into unreadable noise whenever a provider has no token.
  it('leaves the message untouched when the secret is empty', () => {
    expect(redactSecret('403 Forbidden', '')).toBe('403 Forbidden')
  })
})
