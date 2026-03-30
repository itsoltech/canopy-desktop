import { getPref, setPref } from './preferences.svelte'

export interface SidebarSectionDef {
  id: string
  label: string
  forced: boolean
}

export const SECTION_DEFS: SidebarSectionDef[] = [
  { id: 'projects', label: 'Projects', forced: true },
  { id: 'git', label: 'Git', forced: false },
  { id: 'files', label: 'Files', forced: false },
  { id: 'tools', label: 'Tools', forced: false },
]

export interface SidebarSectionConfig {
  id: string
  visible: boolean
}

const PREF_KEY = 'sidebar.sections'

const DEFAULT_CONFIG: SidebarSectionConfig[] = [
  { id: 'projects', visible: true },
  { id: 'git', visible: true },
  { id: 'files', visible: false },
  { id: 'tools', visible: true },
]

export function getSidebarConfig(): SidebarSectionConfig[] {
  const raw = getPref(PREF_KEY, '')
  if (!raw) return DEFAULT_CONFIG
  try {
    const parsed = JSON.parse(raw) as SidebarSectionConfig[]
    const known = SECTION_DEFS.map((d) => d.id)
    const ids = parsed.map((s) => s.id)
    const result = parsed.filter((s) => known.includes(s.id))
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
