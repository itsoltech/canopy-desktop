import type { AgentType } from '../agents/types'

export type { AgentType }

/**
 * Non-secret per-agent settings. Different agents read different keys; the
 * union is intentionally flat so the renderer form can stay simple. Adapters
 * see these via the `${agentType}.<field>` PreferencesReader keys.
 */
export interface ProfilePrefs {
  // Common
  model?: string
  customEnv?: string // raw JSON string
  settingsJson?: string // raw JSON string

  // Claude
  permissionMode?: string
  effortLevel?: string
  appendSystemPrompt?: string
  baseUrl?: string
  provider?: string // 'bedrock' | 'vertex' | 'foundry'
  mcpServers?: string // raw JSON — SDK McpServer map, validated at write time

  // Gemini
  approvalMode?: string

  // Codex
  sandbox?: string
  fullAuto?: string // 'true' | 'false'
  profile?: string
}

/** Internal full record (main process only — never crosses IPC). */
export interface AgentProfile {
  id: string
  agentType: AgentType
  name: string
  isDefault: boolean
  sortIndex: number
  prefs: ProfilePrefs
  apiKey: string | null
  createdAt: string
  updatedAt: string
}

/** Wire format — never includes the decrypted apiKey. */
export interface AgentProfileMasked {
  id: string
  agentType: AgentType
  name: string
  isDefault: boolean
  sortIndex: number
  prefs: ProfilePrefs
  hasApiKey: boolean
  createdAt: string
  updatedAt: string
}

/** Input for save (create or update). */
export interface ProfileInput {
  id?: string
  agentType: AgentType
  name: string
  prefs: ProfilePrefs
  /**
   * Tri-state:
   *  - omitted (undefined) → keep existing apiKey
   *  - null → clear apiKey
   *  - string → set/replace apiKey
   */
  apiKey?: string | null
  sortIndex?: number
}

export const KNOWN_AGENT_TYPES: readonly AgentType[] = [
  'claude',
  'claude-sdk',
  'gemini',
  'opencode',
  'codex',
] as const

/**
 * Per-agent fields the migration reads from the legacy global `preferences`
 * table to seed Default profiles.
 */
export const LEGACY_PREF_FIELDS: Record<AgentType, readonly (keyof ProfilePrefs)[]> = {
  claude: [
    'model',
    'permissionMode',
    'effortLevel',
    'appendSystemPrompt',
    'baseUrl',
    'provider',
    'customEnv',
    'settingsJson',
    'mcpServers',
  ],
  'claude-sdk': [
    'model',
    'permissionMode',
    'appendSystemPrompt',
    'baseUrl',
    'provider',
    'customEnv',
    'mcpServers',
  ],
  gemini: ['model', 'approvalMode', 'customEnv', 'settingsJson'],
  opencode: ['model', 'customEnv', 'settingsJson'],
  codex: [
    'model',
    'approvalMode',
    'sandbox',
    'fullAuto',
    'profile',
    'baseUrl',
    'customEnv',
    'settingsJson',
  ],
} as const
