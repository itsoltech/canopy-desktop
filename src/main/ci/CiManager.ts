import { ResultAsync, errAsync, okAsync } from 'neverthrow'
import type { RepoConfigManager } from '../taskTracker/RepoConfigManager'
import type { KeychainTokenStore } from '../taskTracker/KeychainTokenStore'
import type { CiBuildStatus, CiBuildTypeStatus, CiConfig, CiTriggerResult } from './types'
import type { CiError } from './errors'
import { parseCiConfig } from './config'
import { fetchBuild, fetchBuildForBranch, testConnection, triggerBuild } from './teamcity'

/**
 * Glue between the repo config, the keychain and the TeamCity client. The base URL
 * always comes from the repo's `.canopy/config.json` — the renderer never supplies
 * it, so requests cannot be pointed at another origin (same SSRF stance as the
 * task-tracker providers).
 *
 * This is the single place the raw `ci` block is validated: `RepoConfig.ci` carries
 * the value verbatim (so saves round-trip a malformed block instead of deleting it
 * from the user's git-tracked file), and `parseCiConfig` here decides whether it is
 * usable.
 */
export class CiManager {
  constructor(
    private repoConfigManager: RepoConfigManager,
    private tokenStore: KeychainTokenStore,
  ) {}

  loadConfig(repoRoot: string): ResultAsync<CiConfig, CiError> {
    return this.repoConfigManager
      .load(repoRoot)
      .mapErr((): CiError => ({ _tag: 'CiNotConfigured' }))
      .andThen((cfg) => {
        const ci = parseCiConfig(cfg.ci)
        return ci ? okAsync(ci) : errAsync<CiConfig, CiError>({ _tag: 'CiNotConfigured' })
      })
  }

  private tokenFor(ci: CiConfig): ResultAsync<string, CiError> {
    const creds = this.tokenStore.getCredentials('teamcity', ci.baseUrl)
    return creds?.token
      ? okAsync(creds.token)
      : errAsync({ _tag: 'CiAuthMissing', baseUrl: ci.baseUrl })
  }

  /**
   * Latest build per configured build type for the given branch. Takes an
   * already-loaded config so callers that hold one (the `ci:status` handler polls
   * every 10–45 s) don't pay a second config read per tick.
   */
  statusFor(ci: CiConfig, branch: string): ResultAsync<CiBuildTypeStatus[], CiError> {
    return this.tokenFor(ci).andThen((token) =>
      ResultAsync.combine(
        ci.buildTypes.map((bt) =>
          fetchBuildForBranch(ci.baseUrl, token, bt.id, branch).map((build) => ({
            buildTypeId: bt.id,
            label: bt.label,
            build,
          })),
        ),
      ),
    )
  }

  /** Convenience wrapper for one-shot callers. */
  statusForBranch(repoRoot: string, branch: string): ResultAsync<CiBuildTypeStatus[], CiError> {
    return this.loadConfig(repoRoot).andThen((ci) => this.statusFor(ci, branch))
  }

  trigger(
    repoRoot: string,
    buildTypeId: string,
    branch: string,
  ): ResultAsync<CiTriggerResult, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) => {
      // Only configured build types may be triggered — the renderer cannot queue
      // arbitrary configurations even with a valid id charset.
      if (!ci.buildTypes.some((bt) => bt.id === buildTypeId)) {
        return errAsync<CiTriggerResult, CiError>({
          _tag: 'CiApiError',
          status: 0,
          message: `Build type ${buildTypeId} is not configured for this repository`,
        })
      }
      return this.tokenFor(ci).andThen((token) =>
        triggerBuild(ci.baseUrl, token, buildTypeId, branch),
      )
    })
  }

  build(repoRoot: string, buildId: number): ResultAsync<CiBuildStatus, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) =>
      this.tokenFor(ci).andThen((token) => fetchBuild(ci.baseUrl, token, buildId)),
    )
  }

  /**
   * Connection test for the Settings row. The candidate token comes from the form
   * (not yet stored); the URL still comes from the repo config.
   */
  testConnection(repoRoot: string, token: string): ResultAsync<void, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) => testConnection(ci.baseUrl, token))
  }
}
