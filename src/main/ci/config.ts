import type { CiBuildTypeConfig, CiConfig, CiWorkflowConfig, GitHubActionsCiConfig } from './types'

// Build configuration ids are embedded verbatim in TeamCity locators — the charset
// (TeamCity's own id alphabet) doubles as injection defence for `buildType:(id:…)`.
export const BUILD_TYPE_ID_PATTERN = /^[A-Za-z0-9_]{1,255}$/

// One owner for the bounds: parse (degrade) and ci:saveConfig (reject) must
// enforce the SAME numbers or the two paths drift apart. The renderer keeps a
// mirror in src/renderer/src/lib/ci/limits.ts (it cannot import main modules),
// guarded by a drift test there.
export const CI_MAX_BUILD_TYPES = 50
export const CI_MAX_WORKFLOWS = 50
export const CI_MAX_LABEL_LEN = 100
export const GITHUB_ACTIONS_BASE_URL = 'https://github.com' as const
export const GITHUB_REPOSITORY_PATTERN =
  /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})\/[A-Za-z0-9._-]{1,100}$/
export const GITHUB_WORKFLOW_PATH_PATTERN =
  /^\.github\/workflows\/[A-Za-z0-9][A-Za-z0-9_.-]*\.ya?ml$/i

// The warning copy can only fit a sample — counts stay exact, names are capped.
// Exported: the block-scope reason in CiManager uses the same sample bound.
export const DROPPED_ID_SAMPLE = 10

export function normalizeTeamCityBaseUrl(raw: unknown): string | null {
  if (typeof raw !== 'string' || raw.trim() !== raw || raw.length === 0) return null
  let url: URL
  try {
    url = new URL(raw)
  } catch {
    return null
  }
  if (
    !['http:', 'https:'].includes(url.protocol) ||
    !url.hostname ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    return null
  }
  return url.toString().replace(/\/$/, '')
}

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
  if (o.provider === 'github-actions') return parseGitHubActionsConfig(o)
  if (o.provider !== 'teamcity') return { invalidIds: [] }
  const baseUrl = normalizeTeamCityBaseUrl(o.baseUrl)
  if (!baseUrl) return { invalidIds: [] }

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
  // FULL ids: the configurator matches these against the server's build types to
  // decide which over-cap entries can still be re-ticked. Display truncation
  // belongs at the render site — cutting here would make a long id un-matchable
  // and report a live job as deleted. (invalidIds ARE cut above: they are never
  // compared against anything, only printed.)
  const overCapIds = buildTypes.slice(CI_MAX_BUILD_TYPES).map((bt) => bt.id)
  return {
    invalidIds,
    config: {
      provider: 'teamcity',
      baseUrl,
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

function workflowLabel(path: string): string {
  const filename = path.slice(path.lastIndexOf('/') + 1)
  return filename.replace(/\.ya?ml$/i, '')
}

function parseGitHubActionsConfig(o: Record<string, unknown>): CiConfigParseResult {
  if (typeof o.baseUrl !== 'string') return { invalidIds: [] }
  let url: URL
  try {
    url = new URL(o.baseUrl)
  } catch {
    return { invalidIds: [] }
  }
  if (
    url.protocol !== 'https:' ||
    url.hostname.toLowerCase() !== 'github.com' ||
    url.port ||
    (url.pathname !== '/' && url.pathname !== '') ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    return { invalidIds: [] }
  }
  if (typeof o.repository !== 'string' || !GITHUB_REPOSITORY_PATTERN.test(o.repository)) {
    return { invalidIds: [] }
  }

  const rawWorkflows = Array.isArray(o.workflows) ? o.workflows : []
  const seen = new Set<string>()
  const invalidIds: string[] = []
  const workflows: CiWorkflowConfig[] = rawWorkflows.flatMap((entry) => {
    if (!entry || typeof entry !== 'object') return []
    const { path, label } = entry as Record<string, unknown>
    if (typeof path !== 'string') return []
    if (path.length > 255 || !GITHUB_WORKFLOW_PATH_PATTERN.test(path)) {
      invalidIds.push(path.slice(0, 120))
      return []
    }
    const key = path.toLowerCase()
    if (seen.has(key)) return []
    seen.add(key)
    const trimmed = typeof label === 'string' ? label.trim().slice(0, CI_MAX_LABEL_LEN) : ''
    return [{ path, label: trimmed || workflowLabel(path) }]
  })
  if (workflows.length === 0) return { invalidIds }

  const accepted = workflows.slice(0, CI_MAX_WORKFLOWS)
  const overCapIds = workflows.slice(CI_MAX_WORKFLOWS).map((workflow) => workflow.path)
  const config: GitHubActionsCiConfig = {
    provider: 'github-actions',
    baseUrl: GITHUB_ACTIONS_BASE_URL,
    repository: o.repository.toLowerCase(),
    workflows: accepted,
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
  }
  return { config, invalidIds }
}
