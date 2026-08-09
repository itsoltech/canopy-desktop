import { ResultAsync, err, errAsync, ok, okAsync, type Result } from 'neverthrow'
import type { RepoConfigManager } from '../taskTracker/RepoConfigManager'
import type { KeychainTokenStore } from '../taskTracker/KeychainTokenStore'
import type { CredentialCapability } from '../credentials/CredentialRegistry'
import { taskTrackerErrorMessage } from '../taskTracker/errors'
import { GitRepository } from '../git/GitRepository'
import { parseGitHubRemote } from '../github/remoteUrl'
import type {
  CiActivity,
  CiBuildStatus,
  CiBuildTypeStatus,
  CiConfig,
  CiCredentialStatus,
  CiJobStatus,
  CiParameter,
  CiParameterSet,
  CiRef,
  CiRun,
  CiRunActivity,
  CiRunTriggerResult,
  GitHubActionsSetupInfo,
  CiServerBuildType,
  CiTriggerRequest,
  CiTriggerResult,
  TeamCityCiConfig,
} from './types'
import type { CiError } from './errors'
import { normalizedCredentialToken } from './token'
import { ciErrorMessage } from './errors'
import { DROPPED_ID_SAMPLE, parseCiConfig } from './config'
import {
  fetchActivity,
  fetchBranches,
  fetchBuild,
  fetchBuildForBranch,
  fetchBuildTypes,
  fetchPromptParameters,
  isTeamCityLocatorSafeRef,
  triggerBuild,
} from './teamcity'
import { GitHubActionsClient } from './github-actions/client'
import { discoverGitHubWorkflows, GitHubActionsAdapter } from './providers/github-actions'
import { TeamCityAdapter } from './providers/teamcity'
import type { CiProviderAdapter } from './providers/types'
import { credentialErrorMessage } from '../credentials/errors'
import { githubActionsCredentialBaseUrl } from '../../renderer-shared/credentialBindings'
import { ciDegradedCauses, withCiDegradedCauses } from './degraded'

type RemoteUrlResolver = (repoRoot: string) => ResultAsync<string, unknown>
type GitHubClientFactory = (owner: string, repository: string, token: string) => GitHubActionsClient
export interface CiDispatchConfirmation {
  repository: string
  workflowPath: string
  workflowLabel: string
  ref: CiRef
  inputs: Record<string, string | boolean>
}

export type ConfirmCiDispatch = (details: CiDispatchConfirmation) => Promise<boolean>

function validateConfirmationInputs(
  parameters: CiParameter[],
  inputs: Record<string, string | boolean>,
): Result<void, CiError> {
  const definitions = new Map(parameters.map((parameter) => [parameter.name, parameter]))
  for (const [name, value] of Object.entries(inputs)) {
    const parameter = definitions.get(name)
    if (!parameter) {
      return err({
        _tag: 'CiWorkflowSchemaInvalid',
        reason: `workflow input ${name} is not declared`,
      })
    }
    if (parameter.valueType === 'boolean') {
      if (typeof value !== 'boolean') {
        return err({
          _tag: 'CiWorkflowSchemaInvalid',
          reason: `workflow input ${name} must be boolean`,
        })
      }
      continue
    }
    if (typeof value !== 'string') {
      return err({ _tag: 'CiWorkflowSchemaInvalid', reason: `workflow input ${name} must be text` })
    }
    if (parameter.options && value && !parameter.options.includes(value)) {
      return err({
        _tag: 'CiWorkflowSchemaInvalid',
        reason: `workflow input ${name} is not an allowed option`,
      })
    }
  }
  for (const parameter of parameters) {
    if (!parameter.required || parameter.hasDefault) continue
    const value = inputs[parameter.name]
    if (value === undefined || value === '') {
      return err({
        _tag: 'CiWorkflowSchemaInvalid',
        reason: `workflow input ${parameter.name} is required`,
      })
    }
  }
  return ok(undefined)
}

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
    private remoteUrlResolver: RemoteUrlResolver = (repoRoot) =>
      GitRepository.getRemoteUrl(repoRoot),
    private githubClientFactory: GitHubClientFactory = (owner, repository, token) =>
      new GitHubActionsClient(owner, repository, token),
  ) {}

  /**
   * Records the credential outcome of `result`. `usedSecret` is the token the
   * request was actually built with — redaction must key on THAT, not on whatever
   * is in the store when the response lands: `setCredentials` reuses the credential
   * id for a single-binding credential, so a rotation mid-request would otherwise
   * redact against the new secret and persist the old one verbatim in the reason.
   */
  private observeCredentialResult<T>(
    provider: 'teamcity' | 'github-actions',
    baseUrl: string,
    capability: CredentialCapability,
    result: ResultAsync<T, CiError>,
    usedSecret?: string,
  ): ResultAsync<T, CiError> {
    const record = (status: number, reason?: string, authenticationRejected?: true): void =>
      this.tokenStore.recordResult(provider, baseUrl, capability, status, reason, {
        usedSecret,
        ...(authenticationRejected ? { authenticationRejected } : {}),
      })
    return result
      .map((value) => {
        const degradedCauses = ciDegradedCauses(value)
        if (degradedCauses === undefined || degradedCauses.length === 0) {
          record(200)
        } else {
          const authFailure = degradedCauses.find(
            (cause) =>
              cause._tag === 'CiApiError' && (cause.status === 401 || cause.status === 403),
          )
          if (authFailure?._tag === 'CiApiError') {
            record(authFailure.status, authFailure.message, authFailure.authenticationRejected)
          }
        }
        return value
      })
      .mapErr((error) => {
        if (error._tag === 'CiApiError') {
          record(error.status, error.message, error.authenticationRejected)
        }
        return error
      })
  }

  loadConfig(repoRoot: string): ResultAsync<CiConfig, CiError> {
    return this.repoConfigManager
      .load(repoRoot)
      .mapErr((e): CiError =>
        e._tag === 'ConfigParseError'
          ? { _tag: 'CiConfigInvalid', scope: 'file', reason: e.reason }
          : { _tag: 'CiNotConfigured' },
      )
      .andThen((cfg) => {
        if (cfg.ci == null) return errAsync<CiConfig, CiError>({ _tag: 'CiNotConfigured' })
        const parsed = parseCiConfig(cfg.ci)
        if (parsed.config) return okAsync(parsed.config)
        const provider =
          cfg.ci && typeof cfg.ci === 'object'
            ? (cfg.ci as { provider?: unknown }).provider
            : undefined
        const providerHint =
          provider === 'teamcity' || provider === 'github-actions' ? provider : undefined
        // When EVERY entry is a typo (a bulk rename), the names must still reach
        // the user — a generic "unrecognized shape" would steer them at a Save
        // that deletes the entries with their names never shown.
        return errAsync<CiConfig, CiError>({
          _tag: 'CiConfigInvalid',
          scope: 'block',
          provider: providerHint,
          reason:
            parsed.invalidIds.length > 0
              ? `${providerHint === 'github-actions' ? 'invalid workflow paths' : 'invalid build type ids'} — fix them in the ci block: ${parsed.invalidIds
                  .slice(0, DROPPED_ID_SAMPLE)
                  .join(', ')}`
              : 'unrecognized ci block shape',
        })
      })
  }

  /** Safe credential metadata for the exact provider binding selected by a validated CI config. */
  credentialStatusForConfig(ci: CiConfig): CiCredentialStatus {
    const provider = ci.provider === 'github-actions' ? 'github-actions' : 'teamcity'
    const baseUrl =
      ci.provider === 'github-actions' ? githubActionsCredentialBaseUrl(ci.repository) : ci.baseUrl
    const credentials = this.tokenStore.getCredentials(provider, baseUrl)
    if (!credentials) return { hasToken: false, authenticationState: 'unknown' }
    const descriptor = this.tokenStore.registry
      .list()
      .find((candidate) => candidate.id === credentials.credentialId)
    return {
      hasToken: true,
      authenticationState: descriptor?.authenticationState ?? 'unknown',
      ...(descriptor?.authenticationCheckedAt
        ? { authenticationCheckedAt: descriptor.authenticationCheckedAt }
        : {}),
    }
  }

  private tokenForUrl(
    baseUrl: string,
    capability: Extract<CredentialCapability, 'builds.read' | 'builds.trigger'> = 'builds.read',
  ): ResultAsync<string, CiError> {
    return this.tokenStore.resolveCredentialsResult('teamcity', baseUrl, capability).match(
      (credentials) => {
        const token = normalizedCredentialToken(credentials.token)
        return token
          ? okAsync(token)
          : errAsync({
              _tag: 'CiAuthMissing' as const,
              baseUrl,
            })
      },
      (error) =>
        error._tag === 'CredentialNotFound'
          ? errAsync({ _tag: 'CiAuthMissing' as const, baseUrl })
          : errAsync({
              _tag: 'CiCredentialUnavailable' as const,
              baseUrl,
              reason: credentialErrorMessage(error),
            }),
    )
  }

  private tokenFor(
    ci: TeamCityCiConfig,
    capability: Extract<CredentialCapability, 'builds.read' | 'builds.trigger'> = 'builds.read',
  ): ResultAsync<string, CiError> {
    return this.tokenForUrl(ci.baseUrl, capability)
  }

  private githubToken(
    repository: string,
    capability: Extract<
      CredentialCapability,
      'actions.read' | 'actions.dispatch' | 'contents.read'
    > = 'actions.read',
  ): ResultAsync<string, CiError> {
    const baseUrl = githubActionsCredentialBaseUrl(repository)
    return this.tokenStore.resolveCredentialsResult('github-actions', baseUrl, capability).match(
      (credentials) => okAsync(credentials.token),
      (error) =>
        error._tag === 'CredentialNotFound'
          ? errAsync({
              _tag: 'CiAuthMissing' as const,
              baseUrl,
              provider: 'github-actions' as const,
            })
          : errAsync({
              _tag: 'CiCredentialUnavailable' as const,
              baseUrl,
              provider: 'github-actions' as const,
              reason: credentialErrorMessage(error),
            }),
    )
  }

  private githubClientForWorkspace(
    repoRoot: string,
    candidateToken?: string,
  ): ResultAsync<{ repository: string; client: GitHubActionsClient; token: string }, CiError> {
    return this.remoteUrlResolver(repoRoot)
      .mapErr((): CiError => ({
        _tag: 'CiApiError',
        status: 0,
        message: 'Could not resolve the GitHub remote for this workspace',
        provider: 'github-actions',
      }))
      .andThen((remoteUrl) => {
        const parsed = parseGitHubRemote(remoteUrl)
        if (parsed.isErr() || parsed.value.host.toLowerCase() !== 'github.com') {
          return errAsync<
            { repository: string; client: GitHubActionsClient; token: string },
            CiError
          >({
            _tag: 'CiRepositoryMismatch',
            expected: 'github.com workspace remote',
            actual: parsed.isOk() ? parsed.value.host : 'non-GitHub remote',
          })
        }
        const repository = `${parsed.value.owner}/${parsed.value.repo}`
        const makeClient = (
          token: string,
        ): { repository: string; client: GitHubActionsClient; token: string } => ({
          repository,
          client: this.githubClientFactory(parsed.value.owner, parsed.value.repo, token),
          token,
        })
        return candidateToken
          ? okAsync(makeClient(candidateToken))
          : this.githubToken(repository).map(makeClient)
      })
  }

  private adapterForConfig(
    repoRoot: string,
    ci: CiConfig,
  ): ResultAsync<{ ci: CiConfig; adapter: CiProviderAdapter; token: string }, CiError> {
    if (ci.provider === 'teamcity') {
      return this.tokenFor(ci).map((token) => ({
        ci,
        adapter: new TeamCityAdapter(ci, token),
        token,
      }))
    }

    return this.remoteUrlResolver(repoRoot)
      .mapErr((): CiError => ({
        _tag: 'CiApiError',
        status: 0,
        message: 'Could not resolve the GitHub remote for this workspace',
        provider: 'github-actions',
      }))
      .andThen((remoteUrl) => {
        const parsed = parseGitHubRemote(remoteUrl)
        if (parsed.isErr()) {
          return errAsync<{ ci: CiConfig; adapter: CiProviderAdapter; token: string }, CiError>({
            _tag: 'CiRepositoryMismatch',
            expected: ci.repository,
            actual: 'non-GitHub remote',
          })
        }
        const actual = `${parsed.value.owner}/${parsed.value.repo}`
        if (
          parsed.value.host.toLowerCase() !== 'github.com' ||
          actual.toLowerCase() !== ci.repository.toLowerCase()
        ) {
          return errAsync<{ ci: CiConfig; adapter: CiProviderAdapter; token: string }, CiError>({
            _tag: 'CiRepositoryMismatch',
            expected: ci.repository,
            actual: `${parsed.value.host}/${actual}`,
          })
        }
        return this.githubToken(ci.repository).map((token) => ({
          ci,
          adapter: new GitHubActionsAdapter(
            ci,
            this.githubClientFactory(parsed.value.owner, parsed.value.repo, token),
          ),
          token,
        }))
      })
  }

  private adapter(
    repoRoot: string,
  ): ResultAsync<{ ci: CiConfig; adapter: CiProviderAdapter; token: string }, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) => this.adapterForConfig(repoRoot, ci))
  }

  private validateGitHubWorkspace(
    repoRoot: string,
    expectedRepository: string,
  ): ResultAsync<void, CiError> {
    return this.remoteUrlResolver(repoRoot)
      .mapErr((): CiError => ({
        _tag: 'CiApiError',
        status: 0,
        message: 'Could not resolve the GitHub remote for this workspace',
        provider: 'github-actions',
      }))
      .andThen((remoteUrl) => {
        const parsed = parseGitHubRemote(remoteUrl)
        const actual = parsed.isOk()
          ? `${parsed.value.host}/${parsed.value.owner}/${parsed.value.repo}`
          : 'non-GitHub remote'
        if (
          parsed.isErr() ||
          parsed.value.host.toLowerCase() !== 'github.com' ||
          `${parsed.value.owner}/${parsed.value.repo}`.toLowerCase() !==
            expectedRepository.toLowerCase()
        ) {
          return errAsync<void, CiError>({
            _tag: 'CiRepositoryMismatch',
            expected: expectedRepository,
            actual,
          })
        }
        return okAsync(undefined)
      })
  }

  /**
   * Latest build per configured build type for the given branch. Takes an
   * already-loaded config so callers that hold one (the `ci:status` handler polls
   * every 10–45 s) don't pay a second config read per tick.
   */
  statusFor(ci: CiConfig, branch: string): ResultAsync<CiBuildTypeStatus[], CiError> {
    if (ci.provider !== 'teamcity') {
      return errAsync({
        _tag: 'CiApiError',
        status: 0,
        message: 'Use provider-neutral status for GitHub Actions',
        provider: 'github-actions',
      })
    }
    if (!isTeamCityLocatorSafeRef(branch)) {
      return errAsync({
        _tag: 'CiApiError',
        status: 0,
        message: 'TeamCity branch contains locator-unsafe characters',
      })
    }
    return this.tokenFor(ci).andThen((token) => {
      const causes: CiError[] = []
      const result = ResultAsync.combine(
        ci.buildTypes.map((bt) =>
          fetchBuildForBranch(ci.baseUrl, token, bt.id, branch)
            // One dead build-type id (deleted/re-ided on TeamCity → 404) must cost
            // ONE row, not the whole card: combine() is fail-fast. The failure is
            // CARRIED, not discarded — `null` already means "no build on this
            // branch", so folding an outage into it would claim the branch was
            // never built.
            .map((build): CiBuildTypeStatus => ({ buildTypeId: bt.id, label: bt.label, build }))
            .orElse((e) => {
              causes.push(e)
              return okAsync<CiBuildTypeStatus, CiError>({
                buildTypeId: bt.id,
                label: bt.label,
                build: null,
                error: ciErrorMessage(e),
              })
            }),
        ),
        // A PARTIAL failure counts: `observeCredentialResult` only records a
        // failure for a 401/403 cause, so requiring every row to fail would let a
        // mixed result (one scoped-away build type, the rest fine) re-stamp the
        // credential "verified" on every poll. Same gate as TeamCityAdapter.status.
      ).map((rows) => (causes.length > 0 ? withCiDegradedCauses(rows, causes) : rows))
      return this.observeCredentialResult('teamcity', ci.baseUrl, 'builds.read', result, token)
    })
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
    return this.loadConfig(repoRoot).andThen((ci) => {
      if (ci.provider !== 'teamcity' || !ci.buildTypes.some((bt) => bt.id === buildTypeId)) {
        return errAsync<CiTriggerResult, CiError>({
          _tag: 'CiApiError',
          status: 0,
          message: `Build type ${buildTypeId} is not configured for this repository`,
        })
      }
      return this.tokenFor(ci, 'builds.trigger').andThen((token) =>
        this.observeCredentialResult(
          'teamcity',
          ci.baseUrl,
          'builds.trigger',
          triggerBuild(ci.baseUrl, token, buildTypeId, branch, properties),
          token,
        ),
      )
    })
  }

  /** "Run custom build" prompt parameters of a CONFIGURED build type. */
  promptParameters(repoRoot: string, buildTypeId: string): ResultAsync<CiParameter[], CiError> {
    return this.requireConfiguredBuildType(repoRoot, buildTypeId).andThen(({ ci, token }) =>
      fetchPromptParameters(ci.baseUrl, token, buildTypeId),
    )
  }

  /**
   * Activity limited to build configurations selected in this repository's CI config,
   * and to `branch` when the history window's filter is set to one.
   */
  activity(repoRoot: string, branch?: string): ResultAsync<CiActivity, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) =>
      ci.provider !== 'teamcity'
        ? errAsync<CiActivity, CiError>({
            _tag: 'CiApiError',
            status: 0,
            message: 'Use provider-neutral activity for GitHub Actions',
            provider: 'github-actions',
          })
        : this.tokenFor(ci).andThen((token) => {
            const result = fetchActivity(
              ci.baseUrl,
              token,
              ci.buildTypes.map((bt) => bt.id),
              branch,
            ).map((activity) => {
              const configured = new Set(ci.buildTypes.map((bt) => bt.id))
              const keepConfigured = (build: CiActivity['recent'][number]): boolean =>
                configured.has(build.buildTypeId)
              const filtered: CiActivity = {
                running: activity.running.filter(keepConfigured),
                queued: activity.queued.filter(keepConfigured),
                recent: activity.recent.filter(keepConfigured),
                ...(activity.partialErrors?.length
                  ? { partialErrors: activity.partialErrors }
                  : {}),
              }
              const causes = ciDegradedCauses(activity)
              return causes === undefined ? filtered : withCiDegradedCauses(filtered, causes)
            })
            return this.observeCredentialResult(
              'teamcity',
              ci.baseUrl,
              'builds.read',
              result,
              token,
            )
          }),
    )
  }

  /** Branches TeamCity knows for a CONFIGURED build type — feeds the Run job dialog. */
  branches(repoRoot: string, buildTypeId: string): ResultAsync<string[], CiError> {
    return this.requireConfiguredBuildType(repoRoot, buildTypeId).andThen(({ ci, token }) =>
      fetchBranches(ci.baseUrl, token, buildTypeId),
    )
  }

  // Only configured build types may be queried or triggered — the renderer cannot
  // reach arbitrary configurations even with a valid id charset.
  private requireConfiguredBuildType(
    repoRoot: string,
    buildTypeId: string,
  ): ResultAsync<{ ci: TeamCityCiConfig; token: string }, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) => {
      if (ci.provider !== 'teamcity') {
        return errAsync<{ ci: TeamCityCiConfig; token: string }, CiError>({
          _tag: 'CiApiError',
          status: 0,
          message: 'This endpoint is only available for TeamCity',
          provider: 'github-actions',
        })
      }
      if (!ci.buildTypes.some((bt) => bt.id === buildTypeId)) {
        return errAsync<{ ci: TeamCityCiConfig; token: string }, CiError>({
          _tag: 'CiApiError',
          status: 0,
          message: `Build type ${buildTypeId} is not configured for this repository`,
        })
      }
      return this.tokenFor(ci).map((token): { ci: TeamCityCiConfig; token: string } => ({
        ci,
        token,
      }))
    })
  }

  /**
   * All build configurations on a server — source for the config picker. Runs in the
   * INIT flow, before any `ci` block exists, so the URL comes from the Settings form
   * (validated at the IPC boundary) and the token from the keychain entry the user
   * just saved for that URL.
   */
  listBuildTypes(baseUrl: string): ResultAsync<CiServerBuildType[], CiError> {
    return this.tokenForUrl(baseUrl).andThen((token) =>
      this.observeCredentialResult(
        'teamcity',
        baseUrl,
        'builds.read',
        fetchBuildTypes(baseUrl, token),
        token,
      ),
    )
  }

  // One in-flight update per repo: Save and Remove are both read-modify-write on
  // the same git-shared file, and two overlapping exists→load→save cycles could
  // resurrect a removed block or silently drop a fresh selection. Keyed by the
  // resolved repoRoot the IPC layer passes down.
  private saveChains = new Map<string, Promise<unknown>>()

  /**
   * Write (or remove, with `null`) the `ci` block through the normal repo-config
   * round-trip. Creates `.canopy/config.json` with defaults when the repo has none —
   * same behavior as the Project tracker init flow. Serialized per repo.
   */
  saveConfig(repoRoot: string, ci: CiConfig | null): ResultAsync<void, CiError> {
    return this.enqueueSaveConfig(repoRoot, ci)
  }

  private enqueueSaveConfig(repoRoot: string, ci: CiConfig | null): ResultAsync<void, CiError> {
    const prev = this.saveChains.get(repoRoot) ?? Promise.resolve()
    const run: Promise<Result<void, CiError>> = prev.then(
      () => this.performSaveConfig(repoRoot, ci),
      () => this.performSaveConfig(repoRoot, ci),
    )
    this.saveChains.set(repoRoot, run)
    void run.finally(() => {
      if (this.saveChains.get(repoRoot) === run) this.saveChains.delete(repoRoot)
    })
    return new ResultAsync(run)
  }

  private performSaveConfig(repoRoot: string, ci: CiConfig | null): ResultAsync<void, CiError> {
    const validation =
      ci?.provider === 'github-actions'
        ? this.validateGitHubWorkspace(repoRoot, ci.repository)
        : okAsync<void, CiError>(undefined)
    return validation.andThen(() => this.performConfigWrite(repoRoot, ci))
  }

  private performConfigWrite(repoRoot: string, ci: CiConfig | null): ResultAsync<void, CiError> {
    return ResultAsync.fromSafePromise(this.repoConfigManager.exists(repoRoot))
      .andThen((exists) =>
        // A file that exists but won't load is never initialized over: `init`
        // writes defaults, so ANY read failure — a parse error, EACCES, a
        // transient EMFILE (load's ConfigNotFound tag is lossy about the cause) —
        // would delete the repo's committed trackers, templates and agent config.
        // The filesystem decides whether the file is absent, not the error tag.
        exists ? this.repoConfigManager.load(repoRoot) : this.repoConfigManager.init(repoRoot),
      )
      .andThen((cfg) => this.repoConfigManager.save(repoRoot, { ...cfg, ci: ci ?? undefined }))
      .mapErr((e): CiError => {
        // Same reason loadConfig scopes this: a config file that won't parse is
        // not a TeamCity failure, and CiApiError renders as "TeamCity: …".
        // Neither is a write failure or an unreadable file — exists() already
        // said the file is there, so ConfigNotFound here means EACCES/EMFILE,
        // not absence. NOTHING in this chain talks to TeamCity.
        if (e._tag === 'ConfigParseError') {
          return { _tag: 'CiConfigInvalid', scope: 'file', reason: e.reason }
        }
        return {
          _tag: 'CiConfigUnwritable',
          reason:
            e._tag === 'ConfigNotFound'
              ? 'the existing file could not be read (permissions or a transient file error)'
              : taskTrackerErrorMessage(e),
        }
      })
  }

  build(repoRoot: string, buildId: number): ResultAsync<CiBuildStatus, CiError> {
    return this.loadConfig(repoRoot).andThen((ci) =>
      ci.provider === 'teamcity'
        ? this.tokenFor(ci).andThen((token) => fetchBuild(ci.baseUrl, token, buildId))
        : errAsync<CiBuildStatus, CiError>({
            _tag: 'CiApiError',
            status: 0,
            message: 'Use provider-neutral run lookup for GitHub Actions',
            provider: 'github-actions',
          }),
    )
  }

  jobsStatus(repoRoot: string, ref: CiRef): ResultAsync<CiJobStatus[], CiError> {
    return this.adapter(repoRoot).andThen(({ ci, adapter, token }) =>
      this.observeCredentialResult(
        ci.provider,
        ci.provider === 'github-actions'
          ? githubActionsCredentialBaseUrl(ci.repository)
          : ci.baseUrl,
        ci.provider === 'github-actions' ? 'actions.read' : 'builds.read',
        adapter.status(ref),
        token,
      ),
    )
  }

  jobRefs(repoRoot: string, jobId: string): ResultAsync<CiRef[], CiError> {
    return this.adapter(repoRoot).andThen(({ ci, adapter, token }) =>
      this.observeCredentialResult(
        ci.provider,
        ci.provider === 'github-actions'
          ? githubActionsCredentialBaseUrl(ci.repository)
          : ci.baseUrl,
        ci.provider === 'github-actions' ? 'actions.read' : 'builds.read',
        adapter.refs(jobId),
        token,
      ),
    )
  }

  jobParameters(repoRoot: string, jobId: string, ref: CiRef): ResultAsync<CiParameterSet, CiError> {
    return this.adapter(repoRoot).andThen(({ ci, adapter }) => {
      if (ci.provider === 'github-actions') {
        return this.githubToken(ci.repository, 'contents.read').andThen((contentsToken) =>
          this.observeCredentialResult(
            ci.provider,
            githubActionsCredentialBaseUrl(ci.repository),
            'contents.read',
            adapter.parameters(jobId, ref),
            contentsToken,
          ),
        )
      }
      return this.tokenFor(ci, 'builds.read').andThen((readToken) =>
        this.observeCredentialResult(
          ci.provider,
          ci.baseUrl,
          'builds.read',
          adapter.parameters(jobId, ref),
          readToken,
        ),
      )
    })
  }

  runActivity(repoRoot: string, branch?: string): ResultAsync<CiRunActivity, CiError> {
    return this.adapter(repoRoot).andThen(({ ci, adapter, token }) =>
      this.observeCredentialResult(
        ci.provider,
        ci.provider === 'github-actions'
          ? githubActionsCredentialBaseUrl(ci.repository)
          : ci.baseUrl,
        ci.provider === 'github-actions' ? 'actions.read' : 'builds.read',
        adapter.activity(branch),
        token,
      ),
    )
  }

  runById(repoRoot: string, runId: string): ResultAsync<CiRun, CiError> {
    return this.adapter(repoRoot).andThen(({ ci, adapter, token }) =>
      this.observeCredentialResult(
        ci.provider,
        ci.provider === 'github-actions'
          ? githubActionsCredentialBaseUrl(ci.repository)
          : ci.baseUrl,
        ci.provider === 'github-actions' ? 'actions.read' : 'builds.read',
        adapter.run(runId),
        token,
      ),
    )
  }

  githubSetup(repoRoot: string): ResultAsync<GitHubActionsSetupInfo, CiError> {
    return this.githubClientForWorkspace(repoRoot).andThen(({ repository, client, token }) => {
      const setup = client.verifyAuthentication().andThen(() =>
        client.getRepository().andThen((metadata) => {
          if (metadata.fullName.toLowerCase() !== repository.toLowerCase()) {
            return errAsync<GitHubActionsSetupInfo, CiError>({
              _tag: 'CiRepositoryMismatch',
              expected: repository,
              actual: metadata.fullName,
            })
          }
          return discoverGitHubWorkflows(client, metadata.defaultBranch).map((workflows) => ({
            repository: metadata.fullName.toLowerCase(),
            defaultBranch: metadata.defaultBranch,
            workflows,
          }))
        }),
      )
      return this.observeCredentialResult(
        'github-actions',
        githubActionsCredentialBaseUrl(repository),
        'actions.read',
        setup,
        token,
      )
    })
  }

  testGitHubConnection(repoRoot: string, token: string): ResultAsync<void, CiError> {
    return this.githubClientForWorkspace(repoRoot, token).andThen(({ repository, client }) =>
      client.verifyAuthentication().andThen(() =>
        client.getRepository().andThen((metadata) =>
          metadata.fullName.toLowerCase() === repository.toLowerCase()
            ? okAsync(undefined)
            : errAsync<void, CiError>({
                _tag: 'CiRepositoryMismatch',
                expected: repository,
                actual: metadata.fullName,
              }),
        ),
      ),
    )
  }

  saveGitHubCredential(repoRoot: string, token: string): ResultAsync<void, CiError> {
    return this.githubClientForWorkspace(repoRoot, token).andThen(({ repository }) =>
      this.tokenStore
        .setCredentials('github-actions', githubActionsCredentialBaseUrl(repository), token)
        .map(() => undefined)
        .mapErr((error): CiError => ({
          _tag: 'CiCredentialUnavailable',
          baseUrl: githubActionsCredentialBaseUrl(repository),
          provider: 'github-actions',
          reason: credentialErrorMessage(error),
        })),
    )
  }

  private triggerChains = new Map<string, Promise<Result<CiRunTriggerResult, CiError>>>()

  triggerJob(
    repoRoot: string,
    request: CiTriggerRequest,
    confirm?: ConfirmCiDispatch,
  ): ResultAsync<CiRunTriggerResult, CiError> {
    const orderedInputs = Object.fromEntries(
      Object.entries(request.inputs).sort(([left], [right]) => left.localeCompare(right)),
    )
    const key = JSON.stringify([
      repoRoot,
      request.jobId,
      request.ref,
      request.schemaRevision,
      orderedInputs,
    ])
    const existing = this.triggerChains.get(key)
    if (existing) return new ResultAsync(existing)

    const run = this.performTriggerJob(repoRoot, request, confirm)
    this.triggerChains.set(key, run)
    const clear = (): void => {
      if (this.triggerChains.get(key) === run) this.triggerChains.delete(key)
    }
    void run.then(clear, clear)
    return new ResultAsync(run)
  }

  private async performTriggerJob(
    repoRoot: string,
    request: CiTriggerRequest,
    confirm?: ConfirmCiDispatch,
  ): Promise<Result<CiRunTriggerResult, CiError>> {
    const context = await this.adapter(repoRoot)
    if (context.isErr()) return err(context.error)
    if (context.value.ci.provider === 'github-actions') {
      const dispatchCredential = await this.githubToken(
        context.value.ci.repository,
        'actions.dispatch',
      )
      if (dispatchCredential.isErr()) return err(dispatchCredential.error)
      const refs = await context.value.adapter.refs(request.jobId)
      if (refs.isErr()) return err(refs.error)
      const sameName = refs.value.filter((ref) => ref.name === request.ref.name)
      const resolved = sameName.find((ref) => ref.kind === request.ref.kind)
      if (!resolved || sameName.length !== 1) {
        return err({
          _tag: 'CiApiError',
          status: 0,
          message: `GitHub ref ${request.ref.name} is missing or ambiguous`,
          provider: 'github-actions',
        })
      }
      const parameters = await context.value.adapter.parameters(request.jobId, resolved)
      if (parameters.isErr()) return err(parameters.error)
      if (!request.schemaRevision || parameters.value.schemaRevision !== request.schemaRevision) {
        return err({ _tag: 'CiWorkflowSchemaChanged' })
      }
      const validatedInputs = validateConfirmationInputs(
        parameters.value.parameters,
        request.inputs,
      )
      if (validatedInputs.isErr()) return err(validatedInputs.error)
      const workflow = context.value.ci.workflows.find((item) => item.path === request.jobId)
      if (!workflow)
        return err({ _tag: 'CiWorkflowSchemaInvalid', reason: 'workflow is not configured' })
      // The shared run dialog already provides the confirmation screen. Hosts may still supply
      // a trusted native callback when they want an additional security boundary, but its absence
      // must not turn the in-app confirmation into a silent cancellation.
      if (confirm) {
        let accepted: boolean
        try {
          accepted = await confirm({
            repository: context.value.ci.repository,
            workflowPath: workflow.path,
            workflowLabel: workflow.label,
            ref: resolved,
            inputs: request.inputs,
          })
        } catch {
          return err({
            _tag: 'CiApiError',
            status: 0,
            message: 'Dispatch confirmation failed',
            provider: 'github-actions',
          })
        }
        if (!accepted) return err({ _tag: 'CiDispatchCancelled' })
      }
      request = { ...request, ref: resolved }
    } else {
      const triggerCredential = await this.tokenFor(context.value.ci, 'builds.trigger')
      if (triggerCredential.isErr()) return err(triggerCredential.error)
    }
    const baseUrl =
      context.value.ci.provider === 'github-actions'
        ? githubActionsCredentialBaseUrl(context.value.ci.repository)
        : context.value.ci.baseUrl
    return this.observeCredentialResult(
      context.value.ci.provider,
      baseUrl,
      context.value.ci.provider === 'github-actions' ? 'actions.dispatch' : 'builds.trigger',
      context.value.adapter.trigger(request),
      context.value.token,
    )
  }
}
