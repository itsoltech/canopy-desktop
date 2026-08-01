import { ResultAsync, errAsync, okAsync } from 'neverthrow'
import type { RepoConfigManager } from '../taskTracker/RepoConfigManager'
import type { KeychainTokenStore } from '../taskTracker/KeychainTokenStore'
import { taskTrackerErrorMessage } from '../taskTracker/errors'
import type {
  CiBuildStatus,
  CiBuildTypeStatus,
  CiConfig,
  CiParameter,
  CiServerBuildType,
  CiTriggerResult,
} from './types'
import type { CiError } from './errors'
import { parseCiConfig } from './config'
import {
  fetchBuild,
  fetchBuildForBranch,
  fetchBuildTypes,
  fetchPromptParameters,
  triggerBuild,
} from './teamcity'

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

  private tokenForUrl(baseUrl: string): ResultAsync<string, CiError> {
    const creds = this.tokenStore.getCredentials('teamcity', baseUrl)
    return creds?.token ? okAsync(creds.token) : errAsync({ _tag: 'CiAuthMissing', baseUrl })
  }

  private tokenFor(ci: CiConfig): ResultAsync<string, CiError> {
    return this.tokenForUrl(ci.baseUrl)
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
    properties?: Array<{ name: string; value: string }>,
  ): ResultAsync<CiTriggerResult, CiError> {
    return this.requireConfiguredBuildType(repoRoot, buildTypeId).andThen(({ ci, token }) =>
      triggerBuild(ci.baseUrl, token, buildTypeId, branch, properties),
    )
  }

  /** "Run custom build" prompt parameters of a CONFIGURED build type. */
  promptParameters(repoRoot: string, buildTypeId: string): ResultAsync<CiParameter[], CiError> {
    return this.requireConfiguredBuildType(repoRoot, buildTypeId).andThen(({ ci, token }) =>
      fetchPromptParameters(ci.baseUrl, token, buildTypeId),
    )
  }

  // Only configured build types may be queried or triggered — the renderer cannot
  // reach arbitrary configurations even with a valid id charset.
  private requireConfiguredBuildType(
    repoRoot: string,
    buildTypeId: string,
  ): ResultAsync<{ ci: CiConfig; token: string }, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) => {
      if (!ci.buildTypes.some((bt) => bt.id === buildTypeId)) {
        return errAsync<{ ci: CiConfig; token: string }, CiError>({
          _tag: 'CiApiError',
          status: 0,
          message: `Build type ${buildTypeId} is not configured for this repository`,
        })
      }
      return this.tokenFor(ci).map((token) => ({ ci, token }))
    })
  }

  /**
   * All build configurations on a server — source for the config picker. Runs in the
   * INIT flow, before any `ci` block exists, so the URL comes from the Settings form
   * (validated at the IPC boundary) and the token from the keychain entry the user
   * just saved for that URL.
   */
  listBuildTypes(baseUrl: string): ResultAsync<CiServerBuildType[], CiError> {
    return this.tokenForUrl(baseUrl).andThen((token) => fetchBuildTypes(baseUrl, token))
  }

  /**
   * Write (or remove, with `null`) the `ci` block through the normal repo-config
   * round-trip. Creates `.canopy/config.json` with defaults when the repo has none —
   * same behavior as the Project tracker init flow.
   */
  saveConfig(repoRoot: string, ci: CiConfig | null): ResultAsync<void, CiError> {
    return this.repoConfigManager
      .load(repoRoot)
      .orElse(() => this.repoConfigManager.init(repoRoot))
      .andThen((cfg) => this.repoConfigManager.save(repoRoot, { ...cfg, ci: ci ?? undefined }))
      .mapErr((e): CiError => ({
        _tag: 'CiApiError',
        status: 0,
        message: taskTrackerErrorMessage(e),
      }))
  }

  build(repoRoot: string, buildId: number): ResultAsync<CiBuildStatus, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) =>
      this.tokenFor(ci).andThen((token) => fetchBuild(ci.baseUrl, token, buildId)),
    )
  }
}
