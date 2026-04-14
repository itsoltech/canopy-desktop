import type { AgentType } from '../agents/types'
import type { ProfilePrefs } from '../profiles/types'

export const SETTINGS_EXPORT_VERSION = 1

export interface ExportedProfile {
  agentType: AgentType
  name: string
  isDefault: boolean
  sortIndex: number
  prefs: ProfilePrefs
  apiKey: string | null
}

export interface ExportedCredential {
  domain: string
  username: string
  title: string
  password: string
}

export interface ExportedCustomTool {
  id: string
  name: string
  command: string
  args: string[]
  icon: string
  category: string
}

export interface ExportFile {
  version: typeof SETTINGS_EXPORT_VERSION
  exportedAt: string
  appVersion: string
  preferences: Record<string, string>
  profiles: ExportedProfile[]
  credentials: ExportedCredential[]
  customTools: ExportedCustomTool[]
}

export interface ImportCounts {
  preferences: number
  profiles: number
  credentials: number
  customTools: number
}
