import { createHash } from 'crypto'
import { errAsync, okAsync, ResultAsync } from 'neverthrow'
import type { PreferencesStore } from '../db/PreferencesStore'
import { classifyHttpUrl, type HttpUrlNetworkClass } from '../security/validateUrl'
import type { CiError } from './errors'
import type { TeamCityCiConfig } from './types'

const PRIVATE_ORIGIN_APPROVALS_KEY = 'ci.teamcity.privateOriginApprovals.v1'
const MAX_PRIVATE_ORIGIN_APPROVALS = 200

type OriginClassifier = (url: string) => Promise<HttpUrlNetworkClass>

export interface TeamCityConnectionPolicy {
  /** The actual socket lookup may use a private address only after exact-origin consent. */
  allowPrivate: boolean
}

function normalizedPath(value: string, platform: NodeJS.Platform): string {
  // Callers pass the realpath returned by the workspace gate. Preserve case so distinct
  // case-sensitive repositories cannot collapse to one approval on macOS/Linux. A backslash is
  // a legal POSIX filename character, so treat it as a separator only on Windows.
  const normalized = platform === 'win32' ? value.replace(/\\/g, '/') : value
  return normalized.replace(/\/+$/, '')
}

function normalizedBaseUrl(value: string): string {
  try {
    const url = new URL(value)
    return `${url.origin}${url.pathname}`.replace(/\/$/, '')
  } catch {
    return value.replace(/\/$/, '')
  }
}

function fingerprint(parts: readonly string[]): string {
  // Encode boundaries as data: repository paths can legally contain newlines on Unix.
  return createHash('sha256').update(JSON.stringify(parts)).digest('hex')
}

export function teamCityDiscoveryBindingKey(
  repoRoot: string,
  baseUrl: string,
  platform: NodeJS.Platform = process.platform,
): string {
  return `ci:teamcity:repo-discovery:${fingerprint([
    normalizedPath(repoRoot, platform),
    normalizedBaseUrl(baseUrl),
  ])}`
}

export function teamCityConfigBindingKey(
  repoRoot: string,
  config: TeamCityCiConfig,
  platform: NodeJS.Platform = process.platform,
): string {
  return `ci:teamcity:repo-config:${fingerprint([
    normalizedPath(repoRoot, platform),
    normalizedBaseUrl(config.baseUrl),
    ...config.buildTypes.map((buildType) => buildType.id).sort(),
  ])}`
}

function privateOriginFingerprint(baseUrl: string): string {
  return fingerprint([normalizedBaseUrl(baseUrl)])
}

/** Main-process-only persisted consent for exact private TeamCity URLs. */
export class TeamCityOriginTrust {
  constructor(
    private readonly preferencesStore: Pick<PreferencesStore, 'get' | 'set'>,
    private readonly classify: OriginClassifier = classifyHttpUrl,
  ) {}

  private approvals(): string[] {
    const raw = this.preferencesStore.get(PRIVATE_ORIGIN_APPROVALS_KEY)
    if (!raw) return []
    try {
      const parsed = JSON.parse(raw) as unknown
      if (!Array.isArray(parsed)) return []
      return parsed
        .filter(
          (entry): entry is string => typeof entry === 'string' && /^[a-f0-9]{64}$/.test(entry),
        )
        .slice(-MAX_PRIVATE_ORIGIN_APPROVALS)
    } catch {
      return []
    }
  }

  async requiresApproval(baseUrl: string): Promise<boolean> {
    const networkClass = await this.classify(baseUrl)
    if (networkClass === 'invalid') throw new Error('Invalid TeamCity server URL')
    if (networkClass === 'unresolved') throw new Error('Could not resolve TeamCity server')
    return (
      networkClass === 'private' && !this.approvals().includes(privateOriginFingerprint(baseUrl))
    )
  }

  approve(baseUrl: string): void {
    const approval = privateOriginFingerprint(baseUrl)
    const approvals = this.approvals().filter((entry) => entry !== approval)
    approvals.push(approval)
    this.preferencesStore.set(
      PRIVATE_ORIGIN_APPROVALS_KEY,
      JSON.stringify(approvals.slice(-MAX_PRIVATE_ORIGIN_APPROVALS)),
    )
  }

  ensureAllowed(baseUrl: string): ResultAsync<TeamCityConnectionPolicy, CiError> {
    return ResultAsync.fromPromise(this.classify(baseUrl), (error): CiError => ({
      _tag: 'CiApiError',
      status: 0,
      message: error instanceof Error ? error.message : 'Could not classify TeamCity origin',
    })).andThen((networkClass) => {
      if (networkClass === 'invalid') {
        return errAsync<TeamCityConnectionPolicy, CiError>({
          _tag: 'CiApiError',
          status: 0,
          message: 'Invalid TeamCity server URL',
        })
      }
      if (networkClass === 'unresolved') {
        return errAsync<TeamCityConnectionPolicy, CiError>({
          _tag: 'CiApiError',
          status: 0,
          message: 'Could not resolve TeamCity server',
        })
      }
      const privateApproved = this.approvals().includes(privateOriginFingerprint(baseUrl))
      if (networkClass === 'private' && !privateApproved) {
        return errAsync<TeamCityConnectionPolicy, CiError>({
          _tag: 'CiPrivateOriginApprovalRequired',
          baseUrl,
        })
      }
      return okAsync({ allowPrivate: privateApproved })
    })
  }
}
