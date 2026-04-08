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
      config: global,
      source: {
        branchTemplate: global.branchTemplate ? 'global' : 'default',
        prTemplate: global.prTemplate ? 'global' : 'default',
        filters: 'global',
      },
      hasGlobal: true,
      hasRepo: false,
    }
  }

  // Both exist — additive merge for trackers, repo overrides for other fields
  const g = global!
  const r = repo!

  const trackers = mergeTrackers(g.trackers, r.trackers)

  const branchTemplateSource: ConfigSource | 'default' = r.branchTemplate
    ? 'repo'
    : g.branchTemplate
      ? 'global'
      : 'default'
  const branchTemplate = r.branchTemplate ?? g.branchTemplate

  const prTemplateSource: ConfigSource | 'default' = r.prTemplate
    ? 'repo'
    : g.prTemplate
      ? 'global'
      : 'default'
  const prTemplate = r.prTemplate ?? g.prTemplate

  const repoHasFilters =
    r.filters.statuses.length > 0 || r.filters.assignedToMe !== g.filters.assignedToMe
  const filtersSource: ConfigSource | 'default' = repoHasFilters ? 'repo' : 'global'
  const filters = repoHasFilters ? r.filters : g.filters

  const boardOverrides = { ...g.boardOverrides, ...r.boardOverrides }

  const config: RepoConfig = {
    version: 1,
    trackers,
    branchTemplate,
    prTemplate,
    boardOverrides,
    filters,
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
