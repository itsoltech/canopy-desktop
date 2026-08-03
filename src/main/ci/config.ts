import type { CiBuildTypeConfig, CiConfig } from './types'

// Build configuration ids are embedded verbatim in TeamCity locators — the charset
// (TeamCity's own id alphabet) doubles as injection defence for `buildType:(id:…)`.
export const BUILD_TYPE_ID_PATTERN = /^[A-Za-z0-9_]{1,255}$/

// One owner for the bounds: parse (degrade) and ci:saveConfig (reject) must
// enforce the SAME numbers or the two paths drift apart.
export const CI_MAX_BUILD_TYPES = 50
export const CI_MAX_LABEL_LEN = 100

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
  // becomes an authenticated status fetch on every poll, so the same LIMITS
  // apply — but a hand-edited file degrades instead of being rejected, because
  // `RepoConfigManager` round-trips the raw block and nothing here may destroy
  // it: duplicate ids collapse (first wins) where `ci:saveConfig` doesn't dedupe
  // at all, and the list truncates at the cap where `ci:saveConfig` throws.
  // Labels truncate in both. The truncation is NOT silent: `droppedBuildTypes`
  // carries the count so the configurator can announce it before a Save would
  // delete the invisible entries from the git-tracked file.
  const rawTypes = Array.isArray(o.buildTypes) ? o.buildTypes : []
  const seen = new Set<string>()
  // Ids that carried USER INTENT but will not survive the parse: a string id that
  // fails the charset is a TYPO in a hand-edited file (Gakko-Build for
  // Gakko_Build), and entries beyond the cap are valid but invisible — both are
  // announced by the configurator before a Save deletes them for real.
  // Non-object entries and duplicates lose nothing and are not counted.
  const dropped: string[] = []
  const buildTypes: CiBuildTypeConfig[] = rawTypes.flatMap((entry) => {
    if (!entry || typeof entry !== 'object') return []
    const { id, label } = entry as Record<string, unknown>
    if (typeof id !== 'string') return []
    if (!BUILD_TYPE_ID_PATTERN.test(id)) {
      dropped.push(id.slice(0, 80))
      return []
    }
    if (seen.has(id)) return []
    seen.add(id)
    const trimmed = typeof label === 'string' ? label.trim().slice(0, CI_MAX_LABEL_LEN) : ''
    return [{ id, label: trimmed || id }]
  })
  if (buildTypes.length === 0) return undefined

  const accepted = buildTypes.slice(0, CI_MAX_BUILD_TYPES)
  dropped.push(...buildTypes.slice(CI_MAX_BUILD_TYPES).map((bt) => bt.id))
  return {
    provider: 'teamcity',
    baseUrl: o.baseUrl.replace(/\/$/, ''),
    buildTypes: accepted,
    ...(dropped.length > 0
      ? { droppedBuildTypes: dropped.length, droppedBuildTypeIds: dropped.slice(0, 10) }
      : {}),
  }
}
