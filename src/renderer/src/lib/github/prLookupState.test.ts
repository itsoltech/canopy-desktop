import { describe, expect, it } from 'vitest'
import { pendingPRLookup, settledPRLookup } from './prLookupState'

describe('PR lookup state', () => {
  it('publishes a rejected lookup as one settled state so Retry mounts with the error', () => {
    expect(pendingPRLookup()).toEqual({ loading: true, error: '' })
    expect(settledPRLookup('Failed to check pull requests')).toEqual({
      loading: false,
      error: 'Failed to check pull requests',
    })
  })
})
