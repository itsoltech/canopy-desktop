import type { CiBuildTypeConfig, CiConfig } from './types'

// Build configuration ids are embedded verbatim in TeamCity locators — the charset
// (TeamCity's own id alphabet) doubles as injection defence for `buildType:(id:…)`.
export const BUILD_TYPE_ID_PATTERN = /^[A-Za-z0-9_]{1,255}$/

// One owner for the bounds: parse (degrade) and ci:saveConfig (reject) must
// enforce the SAME numbers or the two paths drift apart. The renderer keeps a
// mirror in src/renderer/src/lib/ci/limits.ts (it cannot import main modules),
// guarded by a drift test there.
export const CI_MAX_BUILD_TYPES = 50
export const CI_MAX_LABEL_LEN = 100

// The warning copy can only fit a sample — counts stay exact, names are capped.
// Exported: the block-scope reason in CiManager uses the same sample bound.
export const DROPPED_ID_SAMPLE = 10

export interface CiConfigParseResult {
  /** Absent when the block cannot be used at all. */
  config?: CiConfig
  /** String ids that failed the charset — typos carrying user intent. Also
      returned when NOTHING survives, so the block-scope error can name them
      instead of a generic "unrecognized shape" (a bulk `_`→`-` rename must not
      end in a Save that deletes every entry with the names never shown). */
  invalidIds: string[]
}

/**
 * Defensive parse of the hand-edited `.canopy/config.json` → `ci` block. An
 * unusable block yields no `config`; `CiManager.loadConfig` turns that into
 * `CiConfigInvalid` (NOT "not configured") so the surfaces that only exist because
 * the block is there report the reason instead of offering to set CI up again.
 * The raw block is never rewritten — `RepoConfigManager` round-trips it verbatim.
 */
export function parseCiConfig(raw: unknown): CiConfigParseResult {
  if (!raw || typeof raw !== 'object') return { invalidIds: [] }
  const o = raw as Record<string, unknown>
  if (o.provider !== 'teamcity') return { invalidIds: [] }
  if (typeof o.baseUrl !== 'string' || !/^https?:\/\//i.test(o.baseUrl)) return { invalidIds: [] }

  // The git-shared file is untrusted input like the IPC payload: every entry
  // becomes an authenticated status fetch on every poll, so the same LIMITS
  // apply — but a hand-edited file degrades instead of being rejected, because
  // `RepoConfigManager` round-trips the raw block and nothing here may destroy
  // it: duplicate ids collapse (first wins) where `ci:saveConfig` doesn't dedupe
  // at all, and the list truncates at the cap where `ci:saveConfig` throws.
  // Labels truncate in both. Neither drop is silent: the two populations have
  // OPPOSITE recoveries (a typo'd id must be corrected in the file — it is not a
  // TeamCity id and can never appear in the picker; an over-cap id is real and
  // can be re-ticked), so they are carried separately for the configurator.
  const rawTypes = Array.isArray(o.buildTypes) ? o.buildTypes : []
  const seen = new Set<string>()
  const invalidIds: string[] = []
  const buildTypes: CiBuildTypeConfig[] = rawTypes.flatMap((entry) => {
    // Non-object entries and id-less objects carry no user intent — not counted.
    if (!entry || typeof entry !== 'object') return []
    const { id, label } = entry as Record<string, unknown>
    if (typeof id !== 'string') return []
    if (!BUILD_TYPE_ID_PATTERN.test(id)) {
      invalidIds.push(id.slice(0, 80))
      return []
    }
    if (seen.has(id)) return []
    seen.add(id)
    const trimmed = typeof label === 'string' ? label.trim().slice(0, CI_MAX_LABEL_LEN) : ''
    return [{ id, label: trimmed || id }]
  })
  if (buildTypes.length === 0) return { invalidIds }

  const accepted = buildTypes.slice(0, CI_MAX_BUILD_TYPES)
  // Same display truncation as invalid ids: a valid id can be 255 chars, and ten
  // of those in a warning paragraph is 2.5k of unbreakable text.
  const overCapIds = buildTypes.slice(CI_MAX_BUILD_TYPES).map((bt) => bt.id.slice(0, 80))
  return {
    invalidIds,
    config: {
      provider: 'teamcity',
      baseUrl: o.baseUrl.replace(/\/$/, ''),
      buildTypes: accepted,
      ...(invalidIds.length > 0
        ? {
            droppedInvalid: {
              count: invalidIds.length,
              ids: invalidIds.slice(0, DROPPED_ID_SAMPLE),
            },
          }
        : {}),
      ...(overCapIds.length > 0
        ? {
            droppedOverCap: {
              count: overCapIds.length,
              ids: overCapIds.slice(0, DROPPED_ID_SAMPLE),
            },
          }
        : {}),
    },
  }
}
