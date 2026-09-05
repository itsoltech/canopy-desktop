import { normalizeCredentialToken } from '../ci/token'
import type { TrackerConfig } from '../taskTracker/types'
import { parseTrackerBindingKey } from '../../renderer-shared/credentialBindings'

export interface KeychainBindingPayload {
  provider: string
  baseUrl: string
  bindingKey?: string
  repoRoot?: string
}

export interface KeychainCredentialPayload extends KeychainBindingPayload {
  token: string
  username?: string
}

const PROVIDER_MAX_LENGTH = 64
const BASE_URL_MAX_LENGTH = 2_048
const REPO_ROOT_MAX_LENGTH = 32_767
const USERNAME_MAX_LENGTH = 320

function requiredText(value: unknown, label: string, maxLength: number): string {
  if (typeof value !== 'string') throw new Error(`${label} is required`)
  const normalized = value.trim()
  if (!normalized) throw new Error(`${label} is required`)
  if (normalized.length > maxLength) throw new Error(`${label} is too long`)
  return normalized
}

function optionalText(value: unknown, label: string, maxLength: number): string | undefined {
  if (value === undefined) return undefined
  if (typeof value !== 'string') throw new Error(`Invalid ${label.toLowerCase()}`)
  const normalized = value.trim()
  if (!normalized) return undefined
  if (normalized.length > maxLength) throw new Error(`${label} is too long`)
  return normalized
}

function comparableBaseUrl(baseUrl: string): string {
  try {
    const parsed = new URL(baseUrl.trim())
    parsed.hash = ''
    parsed.search = ''
    return parsed.toString().replace(/\/+$/, '')
  } catch {
    return baseUrl.trim().replace(/\/+$/, '')
  }
}

/** Authorize a renderer-selected tracker binding against config loaded by the main process. */
export function authorizeKeychainBindingForConfig(
  provider: string,
  baseUrl: string,
  bindingKey: string | undefined,
  trackers: TrackerConfig[],
): void {
  validateKeychainBinding(provider, bindingKey)
  if (!bindingKey) return
  const trackerId = parseTrackerBindingKey(bindingKey)
  if (!trackerId) throw new Error('Invalid credential binding')
  const tracker = trackers.find((candidate) => candidate.id === trackerId)
  if (
    !tracker ||
    tracker.provider !== provider ||
    comparableBaseUrl(tracker.baseUrl) !== comparableBaseUrl(baseUrl)
  ) {
    throw new Error('Credential binding is not authorized for this tracker configuration')
  }
}

export function validateKeychainBinding(provider: string, bindingKey?: string): void {
  if (bindingKey === undefined) return
  if (!['jira', 'youtrack', 'github'].includes(provider) || !parseTrackerBindingKey(bindingKey)) {
    throw new Error('Credential binding does not match the provider purpose')
  }
}

export function normalizeKeychainBindingPayload(raw: unknown): KeychainBindingPayload {
  if (!raw || typeof raw !== 'object') throw new Error('Invalid credential payload')
  const { provider, baseUrl, bindingKey, repoRoot } = raw as Record<string, unknown>
  if (typeof provider !== 'string' || typeof baseUrl !== 'string') {
    throw new Error('Provider and baseUrl are required')
  }
  if (bindingKey !== undefined && (typeof bindingKey !== 'string' || !bindingKey)) {
    throw new Error('Invalid credential binding')
  }
  const normalizedProvider = requiredText(provider, 'Provider', PROVIDER_MAX_LENGTH)
  const normalizedBaseUrl = requiredText(baseUrl, 'Base URL', BASE_URL_MAX_LENGTH)
  const normalizedRepoRoot = optionalText(repoRoot, 'Credential repository', REPO_ROOT_MAX_LENGTH)
  validateKeychainBinding(normalizedProvider, bindingKey)
  return {
    provider: normalizedProvider,
    baseUrl: normalizedBaseUrl,
    bindingKey,
    repoRoot: normalizedRepoRoot,
  }
}

export function normalizeKeychainCredentialPayload(raw: unknown): KeychainCredentialPayload {
  const binding = normalizeKeychainBindingPayload(raw)
  const { token, username } = raw as Record<string, unknown>
  if (typeof token !== 'string') throw new Error('Token is required')
  if (username !== undefined && typeof username !== 'string') {
    throw new Error('Invalid credential username')
  }
  return {
    ...binding,
    token: normalizeCredentialToken(token),
    username: optionalText(username, 'Credential username', USERNAME_MAX_LENGTH),
  }
}
