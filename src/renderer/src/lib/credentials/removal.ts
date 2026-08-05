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
    const count = result.retainedBindings.length
    return `${disconnectedLabel}. Shared credential retained for ${count} other ${count === 1 ? 'connection' : 'connections'}.`
  }
  return result.removed
    ? `${disconnectedLabel}. Stored credential deleted.`
    : `${disconnectedLabel}. No stored credential was present.`
}
