import { describe, expect, it } from 'vitest'
import { credentialRemovalMessage } from './removal'

describe('credentialRemovalMessage', () => {
  it('reports how many connections retain a shared credential without exposing binding keys', () => {
    expect(
      credentialRemovalMessage(
        { removed: false, retainedBindings: ['tracker:jira-main', 'ci:teamcity:tc.example.com'] },
        'Tracker disconnected',
      ),
    ).toBe('Tracker disconnected. Shared credential retained for 2 other connections.')
    expect(
      credentialRemovalMessage(
        { removed: false, retainedBindings: ['tracker:opaque-id'] },
        'Tracker disconnected',
      ),
    ).toBe('Tracker disconnected. Shared credential retained for 1 other connection.')
  })

  it('distinguishes deletion from a missing stored credential', () => {
    expect(
      credentialRemovalMessage({ removed: true, retainedBindings: [] }, 'CI connection removed'),
    ).toBe('CI connection removed. Stored credential deleted.')
    expect(
      credentialRemovalMessage({ removed: false, retainedBindings: [] }, 'CI connection removed'),
    ).toBe('CI connection removed. No stored credential was present.')
  })
})
