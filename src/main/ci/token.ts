export const CI_TOKEN_MAX = 10_000

export function normalizeTeamCityToken(raw: unknown): string {
  if (typeof raw !== 'string' || raw.length > CI_TOKEN_MAX) {
    throw new Error('Invalid TeamCity token')
  }
  const token = raw.trim()
  if (!token) throw new Error('TeamCity token is required')
  return token
}
