import type { CiCredentialStatus } from './types'

export interface TeamCityCredentialGate {
  canLoadJobs: boolean
  credentialLabel: string
  jobsReason: string
  saveReason: string
}

export function canUseTeamCityCredential(
  status: CiCredentialStatus,
  hasReplacementToken: boolean,
): boolean {
  return hasReplacementToken || teamCityCredentialGate(status).canLoadJobs
}

/** User-facing gate for TeamCity job discovery. It never exposes provider or IPC errors. */
export function teamCityCredentialGate(status: CiCredentialStatus): TeamCityCredentialGate {
  if (status.hasToken && status.authenticationState === 'invalid') {
    return {
      canLoadJobs: false,
      credentialLabel: 'TeamCity rejected the stored token',
      jobsReason: 'Update the rejected token under Personal credentials before loading jobs.',
      saveReason: 'Save is disabled until you update the rejected token above.',
    }
  }

  if (!status.hasToken) {
    return {
      canLoadJobs: false,
      credentialLabel: 'No TeamCity token stored',
      jobsReason: 'Add a token under Personal credentials before loading jobs.',
      saveReason: 'Save is disabled until you add a token above.',
    }
  }

  return {
    canLoadJobs: true,
    credentialLabel: 'TeamCity token stored',
    jobsReason: '',
    saveReason: '',
  }
}
