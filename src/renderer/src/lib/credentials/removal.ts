export interface CredentialRemovalResult {
  removed: boolean
  retainedBindings: string[]
}

/** Explain whether disconnecting one integration also deleted its shared secret. */
export function credentialRemovalMessage(
  result: CredentialRemovalResult,
  disconnectedLabel: string,
): string {
  if (result.retainedBindings.length > 0) {
    return `${disconnectedLabel}. Shared credential retained for: ${result.retainedBindings.join(', ')}`
  }
  return result.removed
    ? `${disconnectedLabel}. Stored credential deleted.`
    : `${disconnectedLabel}. No stored credential was present.`
}
