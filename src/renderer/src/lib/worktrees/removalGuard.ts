/**
 * Whether a removal request may proceed without a fresh consent round-trip.
 * Shared rule for every entry point — local flows and the remote RPC host guard:
 * a preflight that demands force must be met by an explicit force flag BEFORE any
 * teardown (tabs, PTYs, selection) starts.
 */
export function removalNeedsForceConsent(
  preflight: { forceRequired: boolean },
  force: boolean,
): boolean {
  return preflight.forceRequired && !force
}
