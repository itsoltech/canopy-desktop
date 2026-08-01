import { ResultAsync, errAsync, okAsync } from 'neverthrow'
import type { RepoConfigManager } from '../taskTracker/RepoConfigManager'
import type { KeychainTokenStore } from '../taskTracker/KeychainTokenStore'
import type { CiBuildStatus, CiBuildTypeStatus, CiConfig, CiError, CiTriggerResult } from './types'
import { fetchBuild, fetchBuildForBranch, testConnection, triggerBuild } from './teamcity'

interface ResolvedCi {
  ci: CiConfig
  token: string
}

/**
 * Glue between the repo config, the keychain and the TeamCity client. The base URL
 * always comes from the repo's `.canopy/config.json` — the renderer never supplies
 * it, so requests cannot be pointed at another origin (same SSRF stance as the
 * task-tracker providers).
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
      .andThen((cfg) =>
        cfg.ci ? okAsync(cfg.ci) : errAsync<CiConfig, CiError>({ _tag: 'CiNotConfigured' }),
      )
  }

  private resolve(repoRoot: string): ResultAsync<ResolvedCi, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) => {
      const creds = this.tokenStore.getCredentials('teamcity', ci.baseUrl)
      if (!creds?.token) {
        return errAsync<ResolvedCi, CiError>({ _tag: 'CiAuthMissing', baseUrl: ci.baseUrl })
      }
      return okAsync({ ci, token: creds.token })
    })
  }

  /** Latest build per configured build type for the given branch. */
  statusForBranch(repoRoot: string, branch: string): ResultAsync<CiBuildTypeStatus[], CiError> {
    return this.resolve(repoRoot).andThen(({ ci, token }) =>
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

  trigger(
    repoRoot: string,
    buildTypeId: string,
    branch: string,
  ): ResultAsync<CiTriggerResult, CiError> {
    return this.resolve(repoRoot).andThen(({ ci, token }) => {
      // Only configured build types may be triggered — the renderer cannot queue
      // arbitrary configurations even with a valid id charset.
      if (!ci.buildTypes.some((bt) => bt.id === buildTypeId)) {
        return errAsync<CiTriggerResult, CiError>({
          _tag: 'CiApiError',
          status: 0,
          message: `Build type ${buildTypeId} is not configured for this repository`,
        })
      }
      return triggerBuild(ci.baseUrl, token, buildTypeId, branch)
    })
  }

  build(repoRoot: string, buildId: number): ResultAsync<CiBuildStatus, CiError> {
    return this.resolve(repoRoot).andThen(({ ci, token }) => fetchBuild(ci.baseUrl, token, buildId))
  }

  /**
   * Connection test for the Settings row. The candidate token comes from the form
   * (not yet stored); the URL still comes from the repo config.
   */
  testConnection(repoRoot: string, token: string): ResultAsync<void, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) => testConnection(ci.baseUrl, token))
  }
}
