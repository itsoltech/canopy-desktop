import { normalizeTeamCityToken } from '../ci/token'

export interface KeychainCredentialPayload {
  provider: string
  baseUrl: string
  token: string
  username?: string
}

export function normalizeKeychainCredentialPayload(raw: unknown): KeychainCredentialPayload {
  if (!raw || typeof raw !== 'object') throw new Error('Invalid credential payload')
  const { provider, baseUrl, token, username } = raw as Record<string, unknown>
  if (typeof provider !== 'string' || !provider || typeof baseUrl !== 'string' || !baseUrl) {
    throw new Error('Provider and baseUrl are required')
  }
  if (typeof token !== 'string') throw new Error('Token is required')
  if (username !== undefined && typeof username !== 'string') {
    throw new Error('Invalid credential username')
  }
  return {
    provider,
    baseUrl,
    token: provider === 'teamcity' ? normalizeTeamCityToken(token) : token,
    username,
  }
}
