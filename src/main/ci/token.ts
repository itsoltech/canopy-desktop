export const CREDENTIAL_TOKEN_MAX = 10_000

export function normalizedCredentialToken(raw: unknown): string | null {
  if (typeof raw !== 'string' || raw.length > CREDENTIAL_TOKEN_MAX) return null
  return raw.trim() || null
}

export function normalizeCredentialToken(raw: unknown): string {
  const token = normalizedCredentialToken(raw)
  if (token) return token
  if (typeof raw !== 'string' || raw.length > CREDENTIAL_TOKEN_MAX)
    throw new Error('Invalid credential token')
  throw new Error('Credential token is required')
}
