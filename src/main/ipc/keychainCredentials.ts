import { normalizeCredentialToken } from '../ci/token'
import type { TrackerConfig } from '../taskTracker/types'

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
  const trackerId = bindingKey.slice('tracker:'.length)
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
  if (
    !['jira', 'youtrack', 'github'].includes(provider) ||
    !/^tracker:[^\r\n]{1,256}$/.test(bindingKey)
  ) {
    throw new Error('Credential binding does not match the provider purpose')
  }
}

export function normalizeKeychainBindingPayload(raw: unknown): KeychainBindingPayload {
  if (!raw || typeof raw !== 'object') throw new Error('Invalid credential payload')
  const { provider, baseUrl, bindingKey, repoRoot } = raw as Record<string, unknown>
  if (typeof provider !== 'string' || !provider || typeof baseUrl !== 'string' || !baseUrl) {
    throw new Error('Provider and baseUrl are required')
  }
  if (bindingKey !== undefined && (typeof bindingKey !== 'string' || !bindingKey)) {
    throw new Error('Invalid credential binding')
  }
  if (repoRoot !== undefined && (typeof repoRoot !== 'string' || !repoRoot)) {
    throw new Error('Invalid credential repository')
  }
  validateKeychainBinding(provider, bindingKey)
  return { provider, baseUrl, bindingKey, repoRoot }
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
    username,
  }
}
