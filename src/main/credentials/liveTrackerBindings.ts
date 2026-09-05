import type { Result } from 'neverthrow'

/**
 * The set of tracker binding keys still referenced by a live configuration —
 * what `KeychainTokenStore.deleteCredentials` prunes against.
 *
 * Extracted from the IPC layer so the FAIL-OPEN contract is testable: returning
 * `undefined` means "the live set could not be established", and the caller must
 * then keep every binding. Unbinding against an incomplete set would delete a
 * shared secret a repository on disk still uses.
 */
export interface LiveTrackerBindingDeps<TrackerLike> {
  /**
   * Trackers from the machine-global config. `undefined` means the config exists but
   * cannot be interpreted, so pruning must fail open instead of treating it as empty.
   */
  globalTrackers: TrackerLike[] | undefined
  /** Persisted workspace paths, already truncated by the store's listing bound. */
  workspacePaths: string[]
  /** Paths open in any window — included even if they fall outside the listing. */
  windowPaths: string[]
  /** Total persisted workspaces. Exceeding `listMax` means `workspacePaths` is a partial view. */
  workspaceCount: number
  listMax: number
  /** True when the repo has a config file; false ONLY for a genuinely absent one. */
  configExists: (repoRoot: string) => Promise<boolean>
  loadTrackers: (repoRoot: string) => Promise<Result<{ trackers: TrackerLike[] }, unknown>>
  bindingKeyFor: (tracker: TrackerLike) => string
}

export async function liveTrackerBindingKeys<TrackerLike>({
  globalTrackers,
  workspacePaths,
  windowPaths,
  workspaceCount,
  listMax,
  configExists,
  loadTrackers,
  bindingKeyFor,
}: LiveTrackerBindingDeps<TrackerLike>): Promise<Set<string> | undefined> {
  if (!globalTrackers) return undefined

  // A truncated listing is an incomplete live set — a binding held by a workspace
  // past the bound would be unbound and its shared secret deleted.
  if (workspaceCount > listMax) return undefined

  const keys = new Set<string>()
  const add = (trackers: TrackerLike[]): void => {
    for (const tracker of trackers) keys.add(bindingKeyFor(tracker))
  }
  add(globalTrackers)

  const paths = new Set([...workspacePaths, ...windowPaths])
  let unknownConfig = false
  await Promise.all(
    [...paths].map(async (repoRoot) => {
      // A repo with no config contributes no trackers and is the NORMAL state —
      // treating that as "unknown" disabled pruning for anyone with a single
      // unconfigured workspace. Only a config that EXISTS and cannot be read or
      // parsed may fail open.
      if (!(await configExists(repoRoot))) return
      const result = await loadTrackers(repoRoot)
      if (result.isOk()) add(result.value.trackers)
      else unknownConfig = true
    }),
  )
  return unknownConfig ? undefined : keys
}
