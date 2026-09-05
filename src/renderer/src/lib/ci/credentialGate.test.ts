import { describe, expect, it } from 'vitest'
import {
  canUseTeamCityCredential,
  credentialConfiguratorSection,
  teamCityCredentialGate,
} from './credentialGate'

describe('teamCityCredentialGate', () => {
  it('routes repository approval to configuration and token recovery to credentials', () => {
    expect(credentialConfiguratorSection(true)).toBeUndefined()
    expect(credentialConfiguratorSection(false)).toBe('credentials')
  })

  it('allows an explicit repository approval prompt for a stored server token', () => {
    expect(
      teamCityCredentialGate({
        hasToken: true,
        approvalRequired: true,
        authenticationState: 'valid',
      }),
    ).toEqual({
      canLoadJobs: true,
      credentialLabel: 'Approval required for this repository',
      jobsReason: '',
      saveReason: '',
    })
  })

  it('blocks job loading and points at token recovery when TeamCity rejected the token', () => {
    expect(teamCityCredentialGate({ hasToken: true, authenticationState: 'invalid' })).toEqual({
      canLoadJobs: false,
      credentialLabel: 'TeamCity rejected the stored token',
      jobsReason: 'Update the rejected token under Personal credentials before loading jobs.',
      saveReason: 'Save is disabled until you update the rejected token above.',
    })
  })

  it('distinguishes a missing token from a rejected token', () => {
    expect(teamCityCredentialGate({ hasToken: false, authenticationState: 'unknown' })).toEqual({
      canLoadJobs: false,
      credentialLabel: 'No TeamCity token stored',
      jobsReason: 'Add a token under Personal credentials before loading jobs.',
      saveReason: 'Save is disabled until you add a token above.',
    })
  })

  it.each(['unknown', 'valid'] as const)(
    'allows a stored token whose authentication state is %s',
    (authenticationState) => {
      expect(teamCityCredentialGate({ hasToken: true, authenticationState })).toEqual({
        canLoadJobs: true,
        credentialLabel: 'TeamCity token stored',
        jobsReason: '',
        saveReason: '',
      })
    },
  )

  it('allows a replacement token to recover a rejected connection during setup', () => {
    expect(canUseTeamCityCredential({ hasToken: true, authenticationState: 'invalid' }, true)).toBe(
      true,
    )
  })
})
