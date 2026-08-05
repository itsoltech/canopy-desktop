import { describe, expect, it } from 'vitest'
import { credentialRemovalMessage } from './removal'

describe('credentialRemovalMessage', () => {
  it('reports the bindings that retain a shared credential', () => {
    expect(
      credentialRemovalMessage(
        { removed: false, retainedBindings: ['tracker:jira-main', 'ci:teamcity:tc.example.com'] },
        'Tracker disconnected',
      ),
    ).toBe(
      'Tracker disconnected. Shared credential retained for: tracker:jira-main, ci:teamcity:tc.example.com',
    )
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
