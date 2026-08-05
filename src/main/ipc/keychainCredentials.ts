import { normalizeCredentialToken } from '../ci/token'

export interface KeychainCredentialPayload {
  provider: string
  baseUrl: string
  token: string
  username?: string
  bindingKey?: string
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

export function normalizeKeychainCredentialPayload(raw: unknown): KeychainCredentialPayload {
  if (!raw || typeof raw !== 'object') throw new Error('Invalid credential payload')
  const { provider, baseUrl, token, username, bindingKey } = raw as Record<string, unknown>
  if (typeof provider !== 'string' || !provider || typeof baseUrl !== 'string' || !baseUrl) {
    throw new Error('Provider and baseUrl are required')
  }
  if (typeof token !== 'string') throw new Error('Token is required')
  if (username !== undefined && typeof username !== 'string') {
    throw new Error('Invalid credential username')
  }
  if (bindingKey !== undefined && (typeof bindingKey !== 'string' || !bindingKey)) {
    throw new Error('Invalid credential binding')
  }
  validateKeychainBinding(provider, bindingKey)
  return {
    provider,
    baseUrl,
    token: normalizeCredentialToken(token),
    username,
    bindingKey,
  }
}
