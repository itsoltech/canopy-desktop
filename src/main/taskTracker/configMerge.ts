import type { RepoConfig, ResolvedConfig, ConfigSource, TrackerConfig } from './types'

function mergeTrackers(global: TrackerConfig[], repo: TrackerConfig[]): TrackerConfig[] {
  const byId = new Map<string, TrackerConfig>()
  for (const t of global) byId.set(t.id, t)
  for (const t of repo) byId.set(t.id, t) // repo wins on same id
  return [...byId.values()]
}

export function mergeConfigs(
  global: RepoConfig | null,
  repo: RepoConfig | null,
): ResolvedConfig | null {
  if (!global && !repo) return null

  // Naming config (branch/PR templates, board overrides) is owned by the PROJECT alone: it comes
  // from the repo config or falls back to the built-in defaults at the call sites. The personal
  // (global) store only contributes tracker connections — it is never a template fallback.
  if (!global && repo) {
    return {
      config: repo,
      source: {
        branchTemplate: repo.branchTemplate ? 'repo' : 'default',
        prTemplate: repo.prTemplate ? 'repo' : 'default',
        filters: 'repo',
      },
      hasGlobal: false,
      hasRepo: true,
    }
  }

  if (global && !repo) {
    return {
      config: {
        version: 1,
        trackers: global.trackers,
        boardOverrides: {},
        filters: global.filters,
      },
      source: {
        branchTemplate: 'default',
        prTemplate: 'default',
        filters: 'global',
      },
      hasGlobal: true,
      hasRepo: false,
    }
  }

  // Both exist — additive merge for trackers only; everything else comes from the repo
  const g = global!
  const r = repo!

  const trackers = mergeTrackers(g.trackers, r.trackers)

  const branchTemplateSource: ConfigSource | 'default' = r.branchTemplate ? 'repo' : 'default'
  const prTemplateSource: ConfigSource | 'default' = r.prTemplate ? 'repo' : 'default'

  // Repo filters always take precedence when repo config exists
  const filtersSource: ConfigSource = 'repo'

  const config: RepoConfig = {
    version: 1,
    trackers,
    branchTemplate: r.branchTemplate,
    prTemplate: r.prTemplate,
    boardOverrides: r.boardOverrides,
    filters: r.filters,
  }

  return {
    config,
    source: {
      branchTemplate: branchTemplateSource,
      prTemplate: prTemplateSource,
      filters: filtersSource,
    },
    hasGlobal: true,
    hasRepo: true,
  }
}
