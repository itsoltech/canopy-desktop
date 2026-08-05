import { randomUUID } from 'crypto'
import { err, ok, type Result } from 'neverthrow'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { CredentialError } from './errors'

const REGISTRY_KEY = 'credential.registry.v2'
const BINDINGS_KEY = 'credential.bindings.v2'
const SECRET_PREFIX = 'credential.secret.v2.'

export type CredentialService = 'github' | 'jira' | 'youtrack' | 'teamcity'
export type CredentialCapability =
  | 'issues.read'
  | 'issues.write'
  | 'actions.read'
  | 'actions.dispatch'
  | 'contents.read'
  | 'builds.read'
  | 'builds.trigger'
  | 'git.push'

export type CredentialIntendedUse = 'tracker' | 'github-actions' | 'teamcity'
export type CapabilityVerificationState = 'unverified' | 'verified' | 'denied'

export interface CredentialAudience {
  host: string
  repository?: string
  baseUrl?: string
}

export interface CapabilityVerification {
  state: CapabilityVerificationState
  checkedAt: string
  reason?: string
}

export interface CredentialDescriptor {
  id: string
  service: CredentialService
  authMethod: string
  audience: CredentialAudience
  intendedUses: CredentialIntendedUse[]
  capabilities: CredentialCapability[]
  account?: string
  createdAt: string
  updatedAt: string
  authenticationState: 'unknown' | 'valid' | 'invalid'
  verification: Partial<Record<CredentialCapability, CapabilityVerification>>
}

export interface SaveCredentialInput {
  id?: string
  service: CredentialService
  authMethod: string
  audience: CredentialAudience
  intendedUses: CredentialIntendedUse[]
  capabilities: CredentialCapability[]
  secret: string
  account?: string
}

export interface ResolveCredentialRequest {
  bindingKey: string
  service: CredentialService
  audience: CredentialAudience
  capability: CredentialCapability
}

export interface ResolvedCredential extends CredentialDescriptor {
  secret: string
}

function normalizedAudience(audience: CredentialAudience): CredentialAudience {
  const host = audience.host.trim().toLowerCase()
  const repository = audience.repository
    ?.trim()
    .replace(/^\/+|\/+$/g, '')
    .toLowerCase()
  const baseUrl = audience.baseUrl?.trim().replace(/\/$/, '')
  return {
    host,
    ...(repository ? { repository } : {}),
    ...(baseUrl ? { baseUrl } : {}),
  }
}

function audienceMatches(stored: CredentialAudience, requested: CredentialAudience): boolean {
  const left = normalizedAudience(stored)
  const right = normalizedAudience(requested)
  if (left.host !== right.host) return false
  if (right.repository && left.repository !== right.repository) return false
  if (right.baseUrl && left.baseUrl !== right.baseUrl) return false
  return true
}

function sanitizedReason(reason?: string, secret?: string): string | undefined {
  if (!reason) return undefined
  const redacted = secret ? reason.replaceAll(secret, '[redacted]') : reason
  return redacted.replace(/[\r\n]+/g, ' ').slice(0, 240)
}

function isCredentialDescriptor(value: unknown): value is CredentialDescriptor {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false
  const record = value as Partial<CredentialDescriptor>
  return (
    typeof record.id === 'string' &&
    typeof record.service === 'string' &&
    typeof record.authMethod === 'string' &&
    !!record.audience &&
    typeof record.audience.host === 'string' &&
    Array.isArray(record.intendedUses) &&
    Array.isArray(record.capabilities) &&
    typeof record.createdAt === 'string' &&
    typeof record.updatedAt === 'string' &&
    typeof record.verification === 'object'
  )
}

/**
 * Main-process-only credential registry. Secrets use one encrypted preference key per stable ID;
 * descriptors and local integration bindings never cross the repository boundary.
 */
export class CredentialRegistry {
  constructor(private preferencesStore: PreferencesStore) {}

  list(): CredentialDescriptor[] {
    const raw = this.preferencesStore.get(REGISTRY_KEY)
    if (!raw) return []
    try {
      const parsed = JSON.parse(raw) as unknown
      return Array.isArray(parsed) ? parsed.filter(isCredentialDescriptor) : []
    } catch {
      return []
    }
  }

  listBindings(): Record<string, string> {
    const raw = this.preferencesStore.get(BINDINGS_KEY)
    if (!raw) return {}
    try {
      const parsed = JSON.parse(raw) as unknown
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {}
      return Object.fromEntries(
        Object.entries(parsed).filter(
          (entry): entry is [string, string] => typeof entry[1] === 'string',
        ),
      )
    } catch {
      return {}
    }
  }

  bindingsFor(credentialId: string): string[] {
    return Object.entries(this.listBindings())
      .filter(([, id]) => id === credentialId)
      .map(([key]) => key)
      .sort()
  }

  save(input: SaveCredentialInput): CredentialDescriptor {
    const now = new Date().toISOString()
    const records = this.list()
    const existing = input.id ? records.find((record) => record.id === input.id) : undefined
    const descriptor: CredentialDescriptor = {
      id: existing?.id ?? randomUUID(),
      service: input.service,
      authMethod: input.authMethod,
      audience: normalizedAudience(input.audience),
      intendedUses: [...new Set(input.intendedUses)],
      capabilities: [...new Set(input.capabilities)],
      ...(input.account ? { account: input.account } : {}),
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
      authenticationState: 'unknown',
      verification: {},
    }
    const next = existing
      ? records.map((record) => (record.id === descriptor.id ? descriptor : record))
      : [...records, descriptor]
    this.preferencesStore.set(SECRET_PREFIX + descriptor.id, input.secret)
    this.preferencesStore.set(REGISTRY_KEY, JSON.stringify(next))
    return descriptor
  }

  bind(bindingKey: string, credentialId: string): Result<void, CredentialError> {
    if (!this.list().some((record) => record.id === credentialId)) {
      return err({ _tag: 'CredentialUnknown', credentialId })
    }
    const bindings = this.listBindings()
    bindings[bindingKey] = credentialId
    this.preferencesStore.set(BINDINGS_KEY, JSON.stringify(bindings))
    return ok(undefined)
  }

  unbind(bindingKey: string): void {
    const bindings = this.listBindings()
    if (!(bindingKey in bindings)) return
    delete bindings[bindingKey]
    this.preferencesStore.set(BINDINGS_KEY, JSON.stringify(bindings))
  }

  resolve(request: ResolveCredentialRequest): Result<ResolvedCredential, CredentialError> {
    const records = this.list()
    const compatible = (record: CredentialDescriptor): boolean =>
      record.service === request.service &&
      audienceMatches(record.audience, request.audience) &&
      record.capabilities.includes(request.capability)

    // Authentication/verification states are diagnostics from the last request, not a second
    // authorization policy. Keeping the bound credential resolvable lets a token recover after
    // permissions are changed server-side and lets the next successful request clear stale 401/403
    // metadata. Service, audience, declared capability and the local binding remain hard gates.

    const boundId = this.listBindings()[request.bindingKey]
    if (boundId) {
      const bound = records.find((record) => record.id === boundId)
      if (!bound) return err({ _tag: 'CredentialUnknown', credentialId: boundId })
      if (!compatible(bound)) {
        return err({ _tag: 'CredentialBindingIncompatible', bindingKey: request.bindingKey })
      }
      const secret = this.preferencesStore.get(SECRET_PREFIX + bound.id)
      return secret
        ? ok({ ...bound, secret })
        : err({ _tag: 'CredentialSecretMissing', credentialId: bound.id })
    }

    const candidates = records.filter(compatible)
    if (candidates.length === 0) return err({ _tag: 'CredentialNotFound' })
    if (candidates.length > 1) {
      return err({ _tag: 'CredentialAmbiguous', candidateCount: candidates.length })
    }
    const [candidate] = candidates
    const secret = this.preferencesStore.get(SECRET_PREFIX + candidate.id)
    if (!secret) return err({ _tag: 'CredentialSecretMissing', credentialId: candidate.id })
    return this.bind(request.bindingKey, candidate.id).map(() => ({ ...candidate, secret }))
  }

  recordCapability(
    credentialId: string,
    capability: CredentialCapability,
    state: CapabilityVerificationState,
    rawReason?: string,
  ): void {
    const records = this.list()
    const now = new Date().toISOString()
    const reason = sanitizedReason(
      rawReason,
      this.preferencesStore.get(SECRET_PREFIX + credentialId) ?? undefined,
    )
    const next = records.map((record): CredentialDescriptor => {
      if (record.id !== credentialId) return record
      return {
        ...record,
        updatedAt: now,
        verification: {
          ...record.verification,
          [capability]: {
            state,
            checkedAt: now,
            ...(reason ? { reason } : {}),
          },
        },
      }
    })
    this.preferencesStore.set(REGISTRY_KEY, JSON.stringify(next))
  }

  recordSuccess(credentialId: string, capability: CredentialCapability): void {
    const records = this.list()
    const current = records.find((record) => record.id === credentialId)
    if (!current) return
    if (
      current.authenticationState === 'valid' &&
      current.verification[capability]?.state === 'verified'
    ) {
      return
    }
    const now = new Date().toISOString()
    this.preferencesStore.set(
      REGISTRY_KEY,
      JSON.stringify(
        records.map((record): CredentialDescriptor =>
          record.id === credentialId
            ? {
                ...record,
                updatedAt: now,
                authenticationState: 'valid',
                verification: {
                  ...record.verification,
                  [capability]: { state: 'verified', checkedAt: now },
                },
              }
            : record,
        ),
      ),
    )
  }

  recordAuthentication(credentialId: string, state: 'valid' | 'invalid'): void {
    const now = new Date().toISOString()
    this.preferencesStore.set(
      REGISTRY_KEY,
      JSON.stringify(
        this.list().map((record) =>
          record.id === credentialId
            ? { ...record, authenticationState: state, updatedAt: now }
            : record,
        ),
      ),
    )
  }

  remove(credentialId: string): { removed: boolean; bindings: string[] } {
    const bindings = this.bindingsFor(credentialId)
    if (bindings.length > 0) return { removed: false, bindings }
    const records = this.list()
    if (!records.some((record) => record.id === credentialId)) {
      return { removed: false, bindings: [] }
    }
    this.preferencesStore.delete(SECRET_PREFIX + credentialId)
    this.preferencesStore.set(
      REGISTRY_KEY,
      JSON.stringify(records.filter((record) => record.id !== credentialId)),
    )
    return { removed: true, bindings: [] }
  }
}
