import { setPref } from './preferences.svelte'

export type SidebarSectionId =
  | 'projects'
  | 'git'
  | 'pullRequests'
  | 'files'
  | 'tools'
  | 'tasks'
  | 'cicd'
  | 'runConfigs'
  | 'remote'

export interface SidebarSectionDef {
  id: SidebarSectionId
  label: string
  forced: boolean
}

export const SECTION_DEFS: SidebarSectionDef[] = [
  { id: 'projects', label: 'Projects', forced: true },
  { id: 'git', label: 'Git - full', forced: false },
  { id: 'pullRequests', label: 'Git - only pull requests', forced: false },
  { id: 'files', label: 'Files', forced: false },
  { id: 'tools', label: 'Tools', forced: false },
  { id: 'tasks', label: 'Project management', forced: false },
  { id: 'cicd', label: 'CI/CD', forced: false },
  { id: 'runConfigs', label: 'Run', forced: false },
  { id: 'remote', label: 'Remote', forced: false },
]

export interface SidebarSectionConfig {
  id: SidebarSectionId
  visible: boolean
}

const PREF_KEY = 'sidebar.sections'

const DEFAULT_CONFIG: SidebarSectionConfig[] = [
  { id: 'projects', visible: true },
  { id: 'git', visible: true },
  { id: 'pullRequests', visible: false },
  { id: 'files', visible: false },
  { id: 'tools', visible: true },
  { id: 'tasks', visible: false },
  // Opt-in (review requirement): the ci block arrives via the git-shared repo config,
  // so the feature must not switch on for everyone the moment a teammate commits it.
  { id: 'cicd', visible: false },
  { id: 'runConfigs', visible: false },
  { id: 'remote', visible: true },
]

export function getSidebarConfig(raw: string): SidebarSectionConfig[] {
  if (!raw) return DEFAULT_CONFIG
  try {
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return DEFAULT_CONFIG
    const validated = parsed.filter(
      (s): s is SidebarSectionConfig =>
        typeof s === 'object' &&
        s !== null &&
        typeof s.id === 'string' &&
        typeof s.visible === 'boolean',
    )
    const known = SECTION_DEFS.map((d) => d.id)
    const ids = validated.map((s) => s.id)
    const result = validated.filter((s) => known.includes(s.id))
    for (const def of SECTION_DEFS) {
      if (!ids.includes(def.id)) {
        const defaultItem = DEFAULT_CONFIG.find((d) => d.id === def.id)
        const item = { id: def.id, visible: defaultItem?.visible ?? false }
        if (def.id === 'pullRequests') {
          const gitIndex = result.findIndex((section) => section.id === 'git')
          result.splice(gitIndex < 0 ? result.length : gitIndex + 1, 0, item)
        } else {
          result.push(item)
        }
      }
    }
    const git = result.find((section) => section.id === 'git')
    const pullRequests = result.find((section) => section.id === 'pullRequests')
    if (git?.visible && pullRequests?.visible) pullRequests.visible = false
    return result
  } catch {
    return DEFAULT_CONFIG
  }
}

export function setSidebarSectionVisibility(
  config: SidebarSectionConfig[],
  id: SidebarSectionId,
  visible: boolean,
): SidebarSectionConfig[] {
  return config.map((item) => {
    if (item.id === id) return { ...item, visible }
    if (
      visible &&
      ((id === 'git' && item.id === 'pullRequests') || (id === 'pullRequests' && item.id === 'git'))
    ) {
      return { ...item, visible: false }
    }
    return item
  })
}

export function saveSidebarConfig(config: SidebarSectionConfig[]): void {
  setPref(PREF_KEY, JSON.stringify(config))
}
