import { setPref } from './preferences.svelte'

export type SidebarSectionId = 'projects' | 'git' | 'files' | 'tools' | 'tasks'

export interface SidebarSectionDef {
  id: SidebarSectionId
  label: string
  forced: boolean
}

export const SECTION_DEFS: SidebarSectionDef[] = [
  { id: 'projects', label: 'Projects', forced: true },
  { id: 'git', label: 'Git', forced: false },
  { id: 'files', label: 'Files', forced: false },
  { id: 'tools', label: 'Tools', forced: false },
  { id: 'tasks', label: 'Tasks', forced: false },
]

export interface SidebarSectionConfig {
  id: SidebarSectionId
  visible: boolean
}

const PREF_KEY = 'sidebar.sections'

const DEFAULT_CONFIG: SidebarSectionConfig[] = [
  { id: 'projects', visible: true },
  { id: 'git', visible: true },
  { id: 'files', visible: false },
  { id: 'tools', visible: true },
  { id: 'tasks', visible: false },
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
        result.push({ id: def.id, visible: defaultItem?.visible ?? false })
      }
    }
    return result
  } catch {
    return DEFAULT_CONFIG
  }
}

export function saveSidebarConfig(config: SidebarSectionConfig[]): void {
  setPref(PREF_KEY, JSON.stringify(config))
}
