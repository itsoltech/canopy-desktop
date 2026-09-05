import type { PreferencesStore } from '../db/PreferencesStore'
import { createHash } from 'crypto'
import { err, ok, type Result } from 'neverthrow'
import {
  CredentialRegistry,
  type CredentialAudience,
  type CredentialCapability,
  type CredentialDescriptor,
  type CredentialIntendedUse,
  type CredentialService,
} from '../credentials/CredentialRegistry'
import type { TrackerConfig } from './types'
import type { TaskTrackerConnection } from './types'
import { parseTrackerBindingKey, trackerBindingKey } from '../../renderer-shared/credentialBindings'
import type { CredentialError } from '../credentials/errors'

export interface TrackerCredentials {
  token: string
  username?: string
  credentialId?: string
}

/** Opaque main-process snapshot pinned to the exact credential secret shown in a consent flow. */
export interface CredentialBindingApprovalTarget {
  credentialId: string
  revision: string
  approvalRequired: boolean
}

/** A stored credential entry WITHOUT the token — safe to cross IPC for Settings. */
export interface StoredTrackerCredential {
  id: string
  provider: string
  baseUrl: string
  username?: string
  service: CredentialService
  intendedUses: CredentialIntendedUse[]
  capabilities: CredentialCapability[]
  verification: CredentialDescriptor['verification']
  authenticationState: CredentialDescriptor['authenticationState']
  /** When that state was decided — the only honest answer to "rejected since when?". */
  authenticationCheckedAt?: string
  bindings: string[]
}

const LEGACY_TOKEN_KEY_PREFIX = 'taskTracker.token.'
const TEAMCITY_REPO_BINDING_PREFIXES = [
  'ci:teamcity:repo-config:',
  'ci:teamcity:repo-discovery:',
] as const

function isTeamCityRepoBinding(bindingKey: string): boolean {
  return TEAMCITY_REPO_BINDING_PREFIXES.some((prefix) => bindingKey.startsWith(prefix))
}

function normalizeUrl(baseUrl: string): string {
  try {
    const url = new URL(baseUrl)
    return (url.origin + url.pathname).replace(/\/$/, '')
  } catch {
    return baseUrl.replace(/\/$/, '')
  }
}

interface ProviderSpec {
  service: CredentialService
  authMethod: string
  intendedUse: CredentialIntendedUse
  capabilities: CredentialCapability[]
  readCapability: CredentialCapability
}

function providerSpec(provider: string): ProviderSpec | null {
  switch (provider) {
    case 'jira':
    case 'youtrack':
      return {
        service: provider,
        authMethod: 'api-token',
        intendedUse: 'tracker',
        capabilities: ['issues.read', 'issues.write'],
        readCapability: 'issues.read',
      }
    case 'github':
      return {
        service: 'github',
        authMethod: 'pat',
        intendedUse: 'tracker',
        capabilities: ['issues.read', 'issues.write'],
        readCapability: 'issues.read',
      }
    case 'github-actions':
      return {
        service: 'github',
        authMethod: 'pat',
        intendedUse: 'github-actions',
        capabilities: ['actions.read', 'contents.read', 'actions.dispatch'],
        readCapability: 'actions.read',
      }
    case 'teamcity':
      return {
        service: 'teamcity',
        authMethod: 'access-token',
        intendedUse: 'teamcity',
        capabilities: ['builds.read', 'builds.trigger'],
        readCapability: 'builds.read',
      }
    default:
      return null
  }
}

function audienceFor(provider: string, baseUrl: string): CredentialAudience {
  const normalized = normalizeUrl(baseUrl)
  try {
    const url = new URL(normalized)
    const path = url.pathname.replace(/^\/+|\/+$/g, '')
    return {
      host: url.host.toLowerCase(),
      ...(provider === 'github-actions' && path ? { repository: path.toLowerCase() } : {}),
      baseUrl: normalized,
    }
  } catch {
    return { host: normalized.toLowerCase(), baseUrl: normalized }
  }
}

function defaultBinding(provider: string, baseUrl: string): string {
  const audience = audienceFor(provider, baseUrl)
  if (provider === 'github-actions') {
    return `ci:github-actions:${audience.host}/${audience.repository ?? ''}`
  }
  // URL parsing already case-folds the host. Preserve the case-sensitive context path so
  // two TeamCity installations cannot collapse onto the same credential binding.
  if (provider === 'teamcity') return `ci:teamcity:${normalizeUrl(baseUrl)}`
  return `credential:shared:tracker:${provider}:${normalizeUrl(baseUrl).toLowerCase()}`
}

function legacyTeamCityBinding(baseUrl: string): string {
  return `ci:teamcity:${normalizeUrl(baseUrl).toLowerCase()}`
}

function credentialRevision(credentials: TrackerCredentials): string {
  return createHash('sha256')
    .update(JSON.stringify([credentials.credentialId, credentials.token]))
    .digest('hex')
}

function descriptorProvider(descriptor: CredentialDescriptor): string {
  if (descriptor.service === 'github' && descriptor.intendedUses.includes('github-actions')) {
    return 'github-actions'
  }
  return descriptor.service
}

/**
 * Purpose-specific facade over the capability registry. Every operation is converted to an
 * explicit local binding + capability. GitHub Actions never falls back to a generic GitHub
 * tracker credential.
 */
export class KeychainTokenStore {
  readonly registry: CredentialRegistry

  constructor(private preferencesStore: PreferencesStore) {
    this.registry = new CredentialRegistry(preferencesStore)
    this.migrateStableCredentials()
  }

  private migrateLegacyTeamCityBinding(
    provider: string,
    baseUrl: string,
    bindingKey: string,
  ): void {
    if (provider !== 'teamcity' || bindingKey !== defaultBinding(provider, baseUrl)) return
    const bindings = this.registry.listBindings()
    const legacyBinding = legacyTeamCityBinding(baseUrl)
    if (legacyBinding === bindingKey) return
    const currentCredentialId = bindings[bindingKey]
    if (currentCredentialId) {
      // A failed second write during an earlier migration may leave both aliases behind.
      // Clean only an alias that points to the same audience-checked credential.
      if (bindings[legacyBinding] === currentCredentialId) this.registry.unbind(legacyBinding)
      return
    }
    const credentialId = bindings[legacyBinding]
    if (!credentialId) return
    const audience = audienceFor(provider, baseUrl)
    const credential = this.registry.list().find((entry) => entry.id === credentialId)
    if (
      !credential ||
      credential.service !== 'teamcity' ||
      credential.audience.host !== audience.host ||
      credential.audience.baseUrl !== audience.baseUrl
    ) {
      return
    }
    this.registry.bind(bindingKey, credentialId)
    this.registry.unbind(legacyBinding)
  }

  listCredentials(): StoredTrackerCredential[] {
    return this.registry.list().map((descriptor) => ({
      id: descriptor.id,
      provider: descriptorProvider(descriptor),
      baseUrl:
        descriptor.audience.baseUrl ??
        `https://${descriptor.audience.host}${descriptor.audience.repository ? `/${descriptor.audience.repository}` : ''}`,
      username: descriptor.account,
      service: descriptor.service,
      intendedUses: descriptor.intendedUses,
      capabilities: descriptor.capabilities,
      verification: descriptor.verification,
      authenticationState: descriptor.authenticationState,
      ...(descriptor.authenticationCheckedAt
        ? { authenticationCheckedAt: descriptor.authenticationCheckedAt }
        : {}),
      bindings: this.registry.bindingsFor(descriptor.id),
    }))
  }

  getCredentials(
    provider: string,
    baseUrl: string,
    bindingKey = defaultBinding(provider, baseUrl),
  ): TrackerCredentials | null {
    const spec = providerSpec(provider)
    if (!spec) return null
    return this.resolveCredentials(provider, baseUrl, spec.readCapability, bindingKey)
  }

  resolveCredentials(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    bindingKey = defaultBinding(provider, baseUrl),
  ): TrackerCredentials | null {
    return this.resolveCredentialsResult(provider, baseUrl, capability, bindingKey).unwrapOr(null)
  }

  resolveCredentialsResult(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    bindingKey = defaultBinding(provider, baseUrl),
  ): Result<TrackerCredentials, CredentialError> {
    const spec = providerSpec(provider)
    if (!spec) return err({ _tag: 'CredentialProviderUnsupported', provider })
    if (!spec.capabilities.includes(capability)) {
      return err({ _tag: 'CredentialCapabilityUnsupported', provider, capability })
    }
    return this.preferencesStore.runInTransaction(() => {
      this.migrateLegacyTeamCityBinding(provider, baseUrl, bindingKey)
      // TeamCity repository approvals can deliberately keep an older credential alive after the
      // server-scoped credential is rotated. If the current server binding is later removed, never
      // let the registry's generic single-candidate fallback silently reactivate that old token.
      if (
        provider === 'teamcity' &&
        bindingKey === defaultBinding(provider, baseUrl) &&
        !this.registry.listBindings()[bindingKey]
      ) {
        return err({ _tag: 'CredentialNotFound' as const })
      }
      const wasBound = this.registry.listBindings()[bindingKey]
      return this.registry
        .resolve({
          bindingKey,
          service: spec.service,
          audience: audienceFor(provider, baseUrl),
          capability,
        })
        .map((resolved) => {
          if (!wasBound && parseTrackerBindingKey(bindingKey)) {
            const sharedBinding = defaultBinding(provider, baseUrl)
            if (this.registry.listBindings()[sharedBinding] === resolved.id) {
              this.registry.unbind(sharedBinding)
            }
          }
          return { token: resolved.secret, username: resolved.account, credentialId: resolved.id }
        })
    })
  }

  /**
   * Resolves only when the current server credential was explicitly bound to this scope.
   * Unlike the generic resolver, this never auto-binds a compatible credential to a repository.
   */
  resolveApprovedCredentialsResult(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    bindingKey: string,
  ): Result<TrackerCredentials, CredentialError> {
    const current = this.resolveCredentialsResult(provider, baseUrl, capability)
    if (current.isErr()) return current
    const approvedCredentialId = this.registry.listBindings()[bindingKey]
    if (!current.value.credentialId || approvedCredentialId !== current.value.credentialId) {
      return err({ _tag: 'CredentialApprovalRequired', bindingKey })
    }
    return this.resolveCredentialsResult(provider, baseUrl, capability, bindingKey)
  }

  isCredentialsBindingApproved(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    bindingKey: string,
  ): boolean {
    return this.resolveApprovedCredentialsResult(provider, baseUrl, capability, bindingKey).isOk()
  }

  /** Captures the exact credential generation before a trusted confirmation is displayed. */
  prepareCredentialsBindingsApproval(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    bindingKeys: readonly string[],
  ): Result<CredentialBindingApprovalTarget, CredentialError> {
    return this.resolveCredentialsResult(provider, baseUrl, capability).map((credentials) => ({
      credentialId: credentials.credentialId!,
      revision: credentialRevision(credentials),
      approvalRequired: bindingKeys.some(
        (bindingKey) => this.registry.listBindings()[bindingKey] !== credentials.credentialId,
      ),
    }))
  }

  /** Called only after a trusted main-process confirmation has accepted the exact scope. */
  approveCredentialsBinding(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    bindingKey: string,
    expected: Pick<CredentialBindingApprovalTarget, 'credentialId' | 'revision'>,
  ): Result<void, CredentialError> {
    return this.approveCredentialsBindings(provider, baseUrl, capability, [bindingKey], expected)
  }

  /** Atomically grants one confirmed credential to every scope named by the caller. */
  approveCredentialsBindings(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    bindingKeys: readonly string[],
    expected: Pick<CredentialBindingApprovalTarget, 'credentialId' | 'revision'>,
  ): Result<void, CredentialError> {
    return this.preferencesStore.runInTransaction(() =>
      this.resolveCredentialsResult(provider, baseUrl, capability).andThen((credentials) => {
        if (!credentials.credentialId) return err({ _tag: 'CredentialNotFound' as const })
        if (
          credentials.credentialId !== expected.credentialId ||
          credentialRevision(credentials) !== expected.revision
        ) {
          return err({
            _tag: 'CredentialApprovalRequired' as const,
            bindingKey: bindingKeys[0] ?? defaultBinding(provider, baseUrl),
          })
        }
        for (const scope of new Set(bindingKeys)) {
          const bound = this.registry.bind(scope, credentials.credentialId)
          if (bound.isErr()) throw new Error('Could not bind approved credential scope')
        }
        return ok(undefined)
      }),
    )
  }

  getCredentialsForTracker(
    tracker: TrackerConfig,
    capability: Extract<CredentialCapability, 'issues.read' | 'issues.write'> = 'issues.read',
  ): TrackerCredentials | null {
    return this.getCredentialsForTrackerResult(tracker, capability).unwrapOr(null)
  }

  getCredentialsForTrackerResult(
    tracker: TrackerConfig,
    capability: Extract<CredentialCapability, 'issues.read' | 'issues.write'> = 'issues.read',
  ): Result<TrackerCredentials, CredentialError> {
    return this.resolveCredentialsResult(
      tracker.provider,
      tracker.baseUrl,
      capability,
      trackerBindingKey(tracker.id),
    )
  }

  setCredentials(
    provider: string,
    baseUrl: string,
    token: string,
    username?: string,
    bindingKey = defaultBinding(provider, baseUrl),
  ): Result<CredentialDescriptor, CredentialError> {
    const spec = providerSpec(provider)
    if (!spec) return err({ _tag: 'CredentialProviderUnsupported', provider })
    return this.preferencesStore.runInTransaction(() => {
      this.migrateLegacyTeamCityBinding(provider, baseUrl, bindingKey)
      const currentlyBoundId = this.registry.listBindings()[bindingKey]
      const currentlyBound = this.registry
        .list()
        .find((credential) => credential.id === currentlyBoundId)
      const audience = audienceFor(provider, baseUrl)
      const compatibleExisting =
        currentlyBound?.service === spec.service &&
        currentlyBound.intendedUses.includes(spec.intendedUse) &&
        currentlyBound.audience.host === audience.host &&
        currentlyBound.audience.repository === audience.repository &&
        currentlyBound.audience.baseUrl === audience.baseUrl
      // Replacing one integration must not silently rotate a secret that another integration uses.
      // Shared credentials fork into a new record and only this binding moves to the replacement.
      const boundId =
        currentlyBoundId &&
        compatibleExisting &&
        this.registry.bindingsFor(currentlyBoundId).length <= 1
          ? currentlyBoundId
          : undefined
      const descriptor = this.registry.save({
        ...(boundId ? { id: boundId } : {}),
        service: spec.service,
        authMethod: spec.authMethod,
        audience,
        intendedUses: [spec.intendedUse],
        capabilities: spec.capabilities,
        secret: token,
        account: username,
      })
      // `bind` can currently fail only with CredentialUnknown. `save` inserted this descriptor in
      // the same transaction, so an Err is unreachable; if bind gains another error path, it must
      // throw here (or be checked before mutation) so SQLite does not commit an Err return value.
      return this.registry.bind(bindingKey, descriptor.id).map(() => {
        if (currentlyBoundId && currentlyBoundId !== descriptor.id) {
          // Repository grants approve one exact TeamCity credential generation. Rotation revokes
          // those derived bindings so the old encrypted secret cannot become an unreachable orphan.
          if (provider === 'teamcity') {
            for (const scope of this.registry.bindingsFor(currentlyBoundId)) {
              if (isTeamCityRepoBinding(scope)) this.registry.unbind(scope)
            }
          }
          if (this.registry.bindingsFor(currentlyBoundId).length === 0) {
            this.registry.remove(currentlyBoundId)
          }
        }
        return descriptor
      })
    })
  }

  deleteCredentials(
    provider: string,
    baseUrl: string,
    bindingKey = defaultBinding(provider, baseUrl),
    liveTrackerBindingKeys?: ReadonlySet<string>,
  ): { removed: boolean; retainedBindings: string[] } {
    return this.preferencesStore.runInTransaction(() => {
      this.migrateLegacyTeamCityBinding(provider, baseUrl, bindingKey)
      const credentialId = this.registry.listBindings()[bindingKey]
      if (!credentialId) return { removed: false, retainedBindings: [] }
      if (provider === 'teamcity') {
        for (const scope of this.registry.bindingsFor(credentialId)) {
          if (isTeamCityRepoBinding(scope)) this.registry.unbind(scope)
        }
      }
      if (liveTrackerBindingKeys) {
        for (const key of this.registry.bindingsFor(credentialId)) {
          if (parseTrackerBindingKey(key) && !liveTrackerBindingKeys.has(key)) {
            this.registry.unbind(key)
          }
        }
      }
      this.registry.unbind(bindingKey)
      const retainedBindings = this.registry.bindingsFor(credentialId)
      if (retainedBindings.length > 0) return { removed: false, retainedBindings }
      return { removed: this.registry.remove(credentialId).removed, retainedBindings: [] }
    })
  }

  hasCredentials(
    provider: string,
    baseUrl: string,
    bindingKey = defaultBinding(provider, baseUrl),
  ): boolean {
    return this.getCredentials(provider, baseUrl, bindingKey) !== null
  }

  /**
   * `opts.usedSecret` is the token the failing request was actually built with.
   * The registry redacts the reason against the secret it holds NOW, which is a
   * different value once a rotation reuses the credential id mid-request — pass
   * the captured one so the old token cannot survive into a stored reason.
   */
  recordResult(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    status: number,
    reason?: string,
    opts?: { bindingKey?: string; usedSecret?: string; authenticationRejected?: true },
  ): void {
    const bindingKey = opts?.bindingKey ?? defaultBinding(provider, baseUrl)
    // Results may arrive after credential rotation revoked a repository grant. Never resolve an
    // absent binding here: the registry's single-candidate convenience path would bind the new
    // credential generation without the native consent required for this repository scope.
    const existingCredentialId = this.registry.listBindings()[bindingKey]
    if (!existingCredentialId) return
    const current = opts?.usedSecret ? this.getCredentials(provider, baseUrl, bindingKey) : null
    // A singly-bound credential keeps its descriptor id when its secret is replaced. Correlate
    // against the actual secret version so a late result cannot validate or reject its successor.
    if (opts?.usedSecret && current?.token !== opts.usedSecret) return
    const credentialId = current?.credentialId ?? existingCredentialId
    if (status === 401 || (status === 403 && opts?.authenticationRejected)) {
      this.registry.recordAuthentication(credentialId, 'invalid')
      return
    }
    if (status === 403) {
      this.registry.recordCapability(credentialId, capability, 'denied', reason, opts?.usedSecret)
      return
    }
    if (status >= 200 && status < 400) {
      this.registry.recordSuccess(credentialId, capability)
    }
  }

  private migrateStableCredentials(): void {
    for (const key of this.preferencesStore.keysWithPrefix(LEGACY_TOKEN_KEY_PREFIX)) {
      const identity = key.slice(LEGACY_TOKEN_KEY_PREFIX.length)
      const separator = identity.indexOf(':')
      if (separator <= 0) continue
      const provider = identity.slice(0, separator)
      const baseUrl = identity.slice(separator + 1)
      if (!providerSpec(provider)) continue
      const trackerBindings = this.trackersFor(provider, baseUrl).map((tracker) =>
        trackerBindingKey(tracker.id),
      )
      const [bindingKey, ...additionalBindings] =
        trackerBindings.length > 0 ? trackerBindings : [defaultBinding(provider, baseUrl)]
      const raw = this.preferencesStore.get(key)
      if (!raw) continue
      let credentials: TrackerCredentials
      try {
        credentials = JSON.parse(raw) as TrackerCredentials
      } catch {
        credentials = { token: raw }
      }
      if (!credentials.token) continue
      const migrated = this.preferencesStore.runInTransaction(() => {
        const descriptor = this.setCredentials(
          provider,
          baseUrl,
          credentials.token,
          credentials.username,
          bindingKey,
        )
        if (descriptor.isErr()) return false
        for (const additionalBinding of additionalBindings) {
          const bound = this.registry.bind(additionalBinding, descriptor.value.id)
          // Returning Err after prior writes would commit a partial migration. This path is
          // unreachable for the descriptor just saved, so fail closed and let SQLite roll back.
          if (bound.isErr()) throw new Error('Could not bind migrated credential')
        }
        this.preferencesStore.delete(key)
        return true
      })
      if (!migrated) continue
    }
  }

  private trackersFor(provider: string, baseUrl: string): TrackerConfig[] {
    const matches = new Map<string, TrackerConfig>()
    const add = (tracker: TrackerConfig): void => {
      if (
        tracker.provider === provider &&
        normalizeUrl(tracker.baseUrl).toLowerCase() === normalizeUrl(baseUrl).toLowerCase()
      ) {
        matches.set(tracker.id, tracker)
      }
    }

    const globalRaw = this.preferencesStore.get('taskTracker.globalConfig')
    if (globalRaw) {
      try {
        const parsed = JSON.parse(globalRaw) as { trackers?: TrackerConfig[] }
        if (Array.isArray(parsed.trackers)) parsed.trackers.forEach(add)
      } catch {
        // Invalid legacy config is handled by GlobalConfigManager; token migration stays safe.
      }
    }

    const connectionsRaw = this.preferencesStore.get('taskTracker.connections')
    if (connectionsRaw) {
      try {
        const connections = JSON.parse(connectionsRaw) as TaskTrackerConnection[]
        for (const connection of connections) {
          add({
            id: connection.id,
            provider: connection.provider,
            baseUrl: connection.baseUrl,
            projectKey: connection.projectKey || undefined,
          })
        }
      } catch {
        // Invalid legacy connections are handled by GlobalConfigManager.
      }
    }

    return [...matches.values()]
  }
}
