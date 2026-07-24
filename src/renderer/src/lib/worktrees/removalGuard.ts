/**
 * Whether a removal request may proceed without a fresh consent round-trip.
 * Shared rule for every entry point — local flows and the remote RPC host guard:
 * a preflight that demands force must be met by an explicit force flag BEFORE any
 * teardown (tabs, PTYs, selection) starts. A missing/failed preflight (`null` —
 * e.g. a ghost worktree whose broken checkout makes `git status` impossible)
 * fails CLOSED: git cannot verify the tree, so it must be treated as
 * force-required rather than waved through.
 */
export function removalNeedsForceConsent(
  preflight: { forceRequired: boolean } | null,
  force: boolean,
): boolean {
  return (preflight?.forceRequired ?? true) && !force
}
