import type { RepoConfig, ResolvedConfig, ConfigSource, TrackerConfig } from './types'

function mergeTrackers(global: TrackerConfig[], repo: TrackerConfig[]): TrackerConfig[] {
  // Personal connections are auto-created for every credential, so the same tracker often exists
  // in both stores (different ids, same provider + URL). The repo entry wins — board overrides
  // and projectKey bind to its id — and the personal duplicate is dropped from the merged view.
  const urlKey = (t: TrackerConfig): string =>
    `${t.provider}:${(t.baseUrl ?? '').replace(/\/$/, '')}`
  const repoUrlKeys = new Set(repo.map(urlKey))
  const byId = new Map<string, TrackerConfig>()
  for (const t of global) {
    if (repoUrlKeys.has(urlKey(t))) continue
    byId.set(t.id, t)
  }
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
      repoTrackerIds: repo.trackers.map((t) => t.id),
    }
  }

  if (global && !repo) {
    return {
      config: {
        version: 1,
        trackers: global.trackers,
        projectOverrides: {},
        filters: global.filters,
      },
      source: {
        branchTemplate: 'default',
        prTemplate: 'default',
        filters: 'global',
      },
      hasGlobal: true,
      hasRepo: false,
      // No repo config — the merged trackers are all personal; none belong to this project.
      repoTrackerIds: [],
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
    projectOverrides: r.projectOverrides,
    filters: r.filters,
    // Agent guidance is project-owned, like the naming templates.
    agents: r.agents,
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
    repoTrackerIds: r.trackers.map((t) => t.id),
  }
}
