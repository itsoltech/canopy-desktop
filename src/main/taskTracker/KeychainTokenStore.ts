import type { PreferencesStore } from '../db/PreferencesStore'
import { err, type Result } from 'neverthrow'
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
  bindings: string[]
}

const LEGACY_TOKEN_KEY_PREFIX = 'taskTracker.token.'

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
  if (provider === 'teamcity') return `ci:teamcity:${normalizeUrl(baseUrl).toLowerCase()}`
  return `credential:shared:tracker:${provider}:${normalizeUrl(baseUrl).toLowerCase()}`
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
    return this.registry.bind(bindingKey, descriptor.id).map(() => {
      if (
        currentlyBoundId &&
        currentlyBoundId !== descriptor.id &&
        this.registry.bindingsFor(currentlyBoundId).length === 0
      ) {
        this.registry.remove(currentlyBoundId)
      }
      return descriptor
    })
  }

  deleteCredentials(
    provider: string,
    baseUrl: string,
    bindingKey = defaultBinding(provider, baseUrl),
    knownTrackerBindingKeys?: ReadonlySet<string>,
  ): { removed: boolean; retainedBindings: string[] } {
    const credentialId = this.registry.listBindings()[bindingKey]
    if (!credentialId) return { removed: false, retainedBindings: [] }
    if (knownTrackerBindingKeys) {
      for (const key of this.registry.bindingsFor(credentialId)) {
        if (parseTrackerBindingKey(key) && !knownTrackerBindingKeys.has(key)) {
          this.registry.unbind(key)
        }
      }
    }
    this.registry.unbind(bindingKey)
    const retainedBindings = this.registry.bindingsFor(credentialId)
    if (retainedBindings.length > 0) return { removed: false, retainedBindings }
    return { removed: this.registry.remove(credentialId).removed, retainedBindings: [] }
  }

  hasCredentials(
    provider: string,
    baseUrl: string,
    bindingKey = defaultBinding(provider, baseUrl),
  ): boolean {
    return this.getCredentials(provider, baseUrl, bindingKey) !== null
  }

  recordResult(
    provider: string,
    baseUrl: string,
    capability: CredentialCapability,
    status: number,
    reason?: string,
    bindingKey = defaultBinding(provider, baseUrl),
  ): void {
    const credentialId = this.registry.listBindings()[bindingKey]
    if (!credentialId) return
    if (status === 401) {
      this.registry.recordAuthentication(credentialId, 'invalid')
      return
    }
    if (status === 403) {
      this.registry.recordCapability(credentialId, capability, 'denied', reason)
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
      const descriptor = this.setCredentials(
        provider,
        baseUrl,
        credentials.token,
        credentials.username,
        bindingKey,
      )
      if (descriptor.isErr()) continue
      let bindingsStored = true
      for (const additionalBinding of additionalBindings) {
        if (this.registry.bind(additionalBinding, descriptor.value.id).isErr()) {
          bindingsStored = false
          break
        }
      }
      if (bindingsStored) this.preferencesStore.delete(key)
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
