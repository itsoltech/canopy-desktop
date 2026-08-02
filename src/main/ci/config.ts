import type { CiBuildTypeConfig, CiConfig } from './types'

// Build configuration ids are embedded verbatim in TeamCity locators — the charset
// (TeamCity's own id alphabet) doubles as injection defence for `buildType:(id:…)`.
export const BUILD_TYPE_ID_PATTERN = /^[A-Za-z0-9_]{1,255}$/

/**
 * Defensive parse of the hand-edited `.canopy/config.json` → `ci` block. Returns
 * `undefined` for anything it cannot use; `CiManager.loadConfig` turns that into
 * `CiConfigInvalid` (NOT "not configured") so the surfaces that only exist because
 * the block is there report the reason instead of offering to set CI up again.
 * The raw block is never rewritten — `RepoConfigManager` round-trips it verbatim.
 */
export function parseCiConfig(raw: unknown): CiConfig | undefined {
  if (!raw || typeof raw !== 'object') return undefined
  const o = raw as Record<string, unknown>
  if (o.provider !== 'teamcity') return undefined
  if (typeof o.baseUrl !== 'string' || !/^https?:\/\//i.test(o.baseUrl)) return undefined

  const rawTypes = Array.isArray(o.buildTypes) ? o.buildTypes : []
  const buildTypes: CiBuildTypeConfig[] = rawTypes.flatMap((entry) => {
    if (!entry || typeof entry !== 'object') return []
    const { id, label } = entry as Record<string, unknown>
    if (typeof id !== 'string' || !BUILD_TYPE_ID_PATTERN.test(id)) return []
    return [{ id, label: typeof label === 'string' && label.trim() ? label : id }]
  })
  if (buildTypes.length === 0) return undefined

  return { provider: 'teamcity', baseUrl: o.baseUrl.replace(/\/$/, ''), buildTypes }
}
