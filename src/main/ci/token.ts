export const CI_TOKEN_MAX = 10_000

export function normalizedTeamCityToken(raw: unknown): string | null {
  if (typeof raw !== 'string' || raw.length > CI_TOKEN_MAX) return null
  return raw.trim() || null
}

export function normalizeTeamCityToken(raw: unknown): string {
  const token = normalizedTeamCityToken(raw)
  if (token) return token
  if (typeof raw !== 'string' || raw.length > CI_TOKEN_MAX)
    throw new Error('Invalid TeamCity token')
  throw new Error('TeamCity token is required')
}
