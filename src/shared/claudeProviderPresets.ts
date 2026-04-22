export type ClaudeCloudProvider = '' | 'bedrock' | 'vertex' | 'foundry'

export type ClaudeProviderPresetId = 'anthropic' | 'kimi' | 'minimax' | 'zai'

export interface ClaudeProviderPreset {
  id: ClaudeProviderPresetId
  label: string
  description: string
  docsUrl?: string
  defaultBaseUrl?: string
  authEnv: 'ANTHROPIC_API_KEY' | 'ANTHROPIC_AUTH_TOKEN'
  notes?: string[]
  defaultProviderModel?: string
  defaultOpusModel?: string
  defaultSonnetModel?: string
  defaultHaikuModel?: string
}

export interface ClaudeProviderPrefsLike {
  model?: string
  baseUrl?: string
  provider?: string
  claudeProviderPreset?: string
  providerModel?: string
  providerOpusModel?: string
  providerSonnetModel?: string
  providerHaikuModel?: string
  customEnv?: string
}

export const CLAUDE_PROVIDER_PRESETS: readonly ClaudeProviderPreset[] = [
  {
    id: 'anthropic',
    label: 'Anthropic',
    description: 'Default Claude Code setup with optional Bedrock, Vertex, or Foundry backend.',
    authEnv: 'ANTHROPIC_API_KEY',
  },
  {
    id: 'kimi',
    label: 'Kimi',
    description: 'Anthropic-compatible Kimi coding endpoint with API key auth.',
    docsUrl: 'https://www.kimi.com/code/docs/en/third-party-agents.html',
    defaultBaseUrl: 'https://api.kimi.com/coding/',
    authEnv: 'ANTHROPIC_API_KEY',
    notes: ['Optional workaround: set ENABLE_TOOL_SEARCH=false if Kimi tool search is incompatible.'],
  },
  {
    id: 'minimax',
    label: 'MiniMax',
    description: 'MiniMax Anthropic-compatible endpoint with Claude model remapping.',
    docsUrl: 'https://platform.minimax.io/docs/coding-plan/claude-code',
    defaultBaseUrl: 'https://api.minimax.io/anthropic',
    authEnv: 'ANTHROPIC_AUTH_TOKEN',
    defaultProviderModel: 'MiniMax-M2.7',
    notes: [
      'China endpoint is https://api.minimaxi.com/anthropic and can be entered manually in Base URL.',
    ],
  },
  {
    id: 'zai',
    label: 'Z.AI',
    description: 'Z.AI GLM coding plan using Anthropic-compatible routing.',
    docsUrl: 'https://docs.z.ai/devpack/tool/claude',
    defaultBaseUrl: 'https://api.z.ai/api/anthropic',
    authEnv: 'ANTHROPIC_AUTH_TOKEN',
    defaultOpusModel: 'GLM-4.7',
    defaultSonnetModel: 'GLM-4.7',
    defaultHaikuModel: 'GLM-4.5-Air',
  },
] as const

export function normalizeClaudeProviderPreset(value: string | null | undefined): ClaudeProviderPresetId {
  const normalized = value?.trim().toLowerCase()
  if (normalized === 'kimi') return 'kimi'
  if (normalized === 'minimax') return 'minimax'
  if (normalized === 'zai') return 'zai'
  return 'anthropic'
}

export function getClaudeProviderPreset(
  value: string | null | undefined,
): ClaudeProviderPreset {
  const id = normalizeClaudeProviderPreset(value)
  return CLAUDE_PROVIDER_PRESETS.find((preset) => preset.id === id) ?? CLAUDE_PROVIDER_PRESETS[0]
}

export function getClaudeProviderBaseUrl(prefs: ClaudeProviderPrefsLike): string | undefined {
  const explicit = prefs.baseUrl?.trim()
  if (explicit) return explicit
  return getClaudeProviderPreset(prefs.claudeProviderPreset).defaultBaseUrl
}

export function providerModelValue(prefs: ClaudeProviderPrefsLike): string | undefined {
  const genericModel = prefs.model?.trim()
  if (genericModel) return genericModel
  const explicit = prefs.providerModel?.trim()
  if (explicit) return explicit
  return getClaudeProviderPreset(prefs.claudeProviderPreset).defaultProviderModel
}

export function providerOpusModelValue(prefs: ClaudeProviderPrefsLike): string | undefined {
  const explicit = prefs.providerOpusModel?.trim()
  if (explicit) return explicit
  return getClaudeProviderPreset(prefs.claudeProviderPreset).defaultOpusModel
}

export function providerSonnetModelValue(prefs: ClaudeProviderPrefsLike): string | undefined {
  const explicit = prefs.providerSonnetModel?.trim()
  if (explicit) return explicit
  return getClaudeProviderPreset(prefs.claudeProviderPreset).defaultSonnetModel
}

export function providerHaikuModelValue(prefs: ClaudeProviderPrefsLike): string | undefined {
  const explicit = prefs.providerHaikuModel?.trim()
  if (explicit) return explicit
  return getClaudeProviderPreset(prefs.claudeProviderPreset).defaultHaikuModel
}
