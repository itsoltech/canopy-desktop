import type { PreferencesStore } from '../db/PreferencesStore'
import type { KeychainTokenStore } from './KeychainTokenStore'
import type { RepoConfig, BranchTemplateConfig, PRTemplateConfig } from './types'
import type { TaskTrackerConnection } from './types'
import { getBranchTemplate, getPRTemplate } from './configDefaults'

const GLOBAL_CONFIG_KEY = 'taskTracker.globalConfig'
const MIGRATION_FLAG_KEY = 'taskTracker.migratedToGlobalConfig'

export class GlobalConfigManager {
  private cached: RepoConfig | null = null
  private cacheValid = false

  constructor(
    private preferencesStore: PreferencesStore,
    private keychainTokenStore: KeychainTokenStore,
  ) {}

  load(): RepoConfig | null {
    if (this.cacheValid) return this.cached
    this.migrateIfNeeded()
    const raw = this.preferencesStore.get(GLOBAL_CONFIG_KEY)
    if (!raw) {
      this.cached = null
      this.cacheValid = true
      return null
    }
    try {
      const parsed = JSON.parse(raw) as Record<string, unknown>

      const VALID_PROVIDERS = new Set(['jira', 'youtrack', 'github'])

      // Backward compat: convert old single `tracker` to `trackers` array
      if (!parsed.trackers && parsed.tracker) {
        const old = parsed.tracker as { provider: string; baseUrl: string }
        if (!VALID_PROVIDERS.has(old.provider)) {
          this.cached = null
          this.cacheValid = true
          return null
        }
        const config: RepoConfig = {
          version: 1,
          trackers: [
            {
              id: `${old.provider}-default`,
              provider: old.provider as RepoConfig['trackers'][0]['provider'],
              baseUrl: old.baseUrl,
            },
          ],
          branchTemplate: parsed.branchTemplate as RepoConfig['branchTemplate'],
          prTemplate: parsed.prTemplate as RepoConfig['prTemplate'],
          boardOverrides: (parsed.boardOverrides ?? {}) as RepoConfig['boardOverrides'],
          filters: (parsed.filters ?? {
            assignedToMe: true,
            statuses: [],
          }) as RepoConfig['filters'],
        }
        this.save(config)
        return config
      }

      // Validate minimum structure before returning
      if (!Array.isArray(parsed.trackers) || !parsed.filters) {
        this.cached = null
        this.cacheValid = true
        return null
      }

      const result: RepoConfig = {
        version: 1,
        trackers: parsed.trackers as RepoConfig['trackers'],
        branchTemplate: parsed.branchTemplate as RepoConfig['branchTemplate'],
        prTemplate: parsed.prTemplate as RepoConfig['prTemplate'],
        boardOverrides: (parsed.boardOverrides ?? {}) as RepoConfig['boardOverrides'],
        filters: parsed.filters as RepoConfig['filters'],
      }
      this.cached = result
      this.cacheValid = true
      return result
    } catch {
      this.cached = null
      this.cacheValid = true
      return null
    }
  }

  save(config: RepoConfig): void {
    this.preferencesStore.set(GLOBAL_CONFIG_KEY, JSON.stringify(config))
    this.cached = config
    this.cacheValid = true
  }

  exists(): boolean {
    return this.load() !== null
  }

  getBranchTemplate(
    config: RepoConfig,
    boardId?: string,
  ): BranchTemplateConfig & { typeMapping?: Record<string, string> } {
    return getBranchTemplate(config, boardId)
  }

  getPRTemplate(config: RepoConfig, boardId?: string): PRTemplateConfig {
    return getPRTemplate(config, boardId)
  }

  private migrateIfNeeded(): void {
    if (this.preferencesStore.get(MIGRATION_FLAG_KEY)) return
    if (this.preferencesStore.get(GLOBAL_CONFIG_KEY)) {
      this.preferencesStore.set(MIGRATION_FLAG_KEY, '1')
      return
    }

    const connectionsRaw = this.preferencesStore.get('taskTracker.connections')
    if (!connectionsRaw) {
      this.preferencesStore.set(MIGRATION_FLAG_KEY, '1')
      return
    }

    try {
      const connections = JSON.parse(connectionsRaw) as TaskTrackerConnection[]
      if (connections.length === 0) {
        this.preferencesStore.set(MIGRATION_FLAG_KEY, '1')
        return
      }

      // Migrate ALL legacy connections into trackers array
      const trackers: RepoConfig['trackers'] = connections.map((c) => ({
        id: `${c.provider}-${c.id.slice(0, 8)}`,
        provider: c.provider,
        baseUrl: c.baseUrl,
        projectKey: c.projectKey || undefined,
      }))

      const first = connections[0]
      const config: RepoConfig = {
        version: 1,
        trackers,
        boardOverrides: {},
        filters: {
          assignedToMe: true,
          statuses: [],
        },
      }

      // Migrate branch template from legacy prefs
      const branchRaw =
        this.preferencesStore.get(`taskTracker.branchTemplate.${first.id}`) ||
        this.preferencesStore.get('taskTracker.branchTemplate')
      if (branchRaw) {
        try {
          const parsed = JSON.parse(branchRaw)
          if (parsed.template) {
            config.branchTemplate = {
              template: parsed.template,
              customVars: parsed.customVars ?? {},
            }
          }
        } catch {
          // skip
        }
      }

      // Migrate PR template from legacy prefs
      const prRaw =
        this.preferencesStore.get(`taskTracker.pr.${first.id}`) ||
        this.preferencesStore.get('taskTracker.pr')
      if (prRaw) {
        try {
          const parsed = JSON.parse(prRaw)
          if (parsed.titleTemplate) {
            config.prTemplate = {
              titleTemplate: parsed.titleTemplate,
              bodyTemplate: parsed.bodyTemplate ?? '## {taskKey}: {taskTitle}\n\n{taskUrl}',
              defaultTargetBranch: parsed.defaultBranch ?? '',
              targetRules: parsed.targetRules ?? [],
            }
          }
        } catch {
          // skip
        }
      }

      this.save(config)

      // Migrate ALL tokens: old key (taskTracker.token.{uuid}) → new key (provider:baseUrl)
      for (const conn of connections) {
        const oldToken = this.preferencesStore.get(conn.authPrefKey)
        if (oldToken && conn.baseUrl) {
          const existingCreds = this.keychainTokenStore.getCredentials(conn.provider, conn.baseUrl)
          if (!existingCreds) {
            this.keychainTokenStore.setCredentials(
              conn.provider,
              conn.baseUrl,
              oldToken,
              conn.username,
            )
          }
          // Delete old plaintext token from preferences
          this.preferencesStore.delete(conn.authPrefKey)
        }
      }
    } catch (e) {
      console.error('[GlobalConfigManager] Legacy migration failed:', e)
    }
    this.preferencesStore.set(MIGRATION_FLAG_KEY, '1')
  }
}
