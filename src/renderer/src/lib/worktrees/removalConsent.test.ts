import { describe, expect, it } from 'vitest'
import { removalNeedsForceConsent } from './removalGuard'

describe('removalNeedsForceConsent', () => {
  it('blocks a force-required removal without the force flag (host guard for remote callers)', () => {
    expect(removalNeedsForceConsent({ forceRequired: true }, false)).toBe(true)
  })

  it('allows a force-required removal once force consent was collected', () => {
    expect(removalNeedsForceConsent({ forceRequired: true }, true)).toBe(false)
  })

  it('allows a clean removal without force', () => {
    expect(removalNeedsForceConsent({ forceRequired: false }, false)).toBe(false)
  })
})
