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

  // The git-shared file is untrusted input like the IPC payload: every entry
  // becomes an authenticated status fetch on every poll, so the SAME bounds as
  // ci:saveConfig apply here — duplicates collapse (first wins), the list is
  // capped at 50, labels at 100 chars. A hand-edited file with thousands of
  // copies of one valid id must not fan out unbounded requests.
  const rawTypes = Array.isArray(o.buildTypes) ? o.buildTypes : []
  const seen = new Set<string>()
  const buildTypes: CiBuildTypeConfig[] = rawTypes.flatMap((entry) => {
    if (!entry || typeof entry !== 'object') return []
    const { id, label } = entry as Record<string, unknown>
    if (typeof id !== 'string' || !BUILD_TYPE_ID_PATTERN.test(id)) return []
    if (seen.has(id)) return []
    seen.add(id)
    const trimmed = typeof label === 'string' ? label.trim().slice(0, 100) : ''
    return [{ id, label: trimmed || id }]
  })
  if (buildTypes.length === 0) return undefined

  return {
    provider: 'teamcity',
    baseUrl: o.baseUrl.replace(/\/$/, ''),
    buildTypes: buildTypes.slice(0, 50),
  }
}
