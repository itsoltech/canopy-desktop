import { BLOCKED_ENV_VARS } from '../security/envBlocklist'
import {
  getClaudeProviderBaseUrl,
  getClaudeProviderPreset,
  providerHaikuModelValue,
  providerModelValue,
  providerOpusModelValue,
  providerSonnetModelValue,
  type ClaudeProviderPrefsLike,
} from '../../shared/claudeProviderPresets'

const INTERNAL_BLOCKED = new Set(['CANOPY_HOOK_PORT', 'CANOPY_HOOK_TOKEN', 'ELECTRON_RUN_AS_NODE'])

function mergeCustomEnv(target: Record<string, string>, raw: string | undefined): void {
  if (!raw) return
  try {
    const parsed = JSON.parse(raw)
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return
    for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) {
      if (
        typeof v === 'string' &&
        !BLOCKED_ENV_VARS.has(k.toUpperCase()) &&
        !INTERNAL_BLOCKED.has(k)
      ) {
        target[k] = v
      }
    }
  } catch {
    // Invalid JSON is ignored; form validation owns user feedback.
  }
}

export function buildClaudeProviderEnv(
  apiKey: string | null,
  prefs: ClaudeProviderPrefsLike,
): Record<string, string> {
  const preset = getClaudeProviderPreset(prefs.claudeProviderPreset)
  const env: Record<string, string> = {
    ANTHROPIC_BASE_URL: '',
    CLAUDE_CODE_USE_BEDROCK: '',
    CLAUDE_CODE_USE_VERTEX: '',
    CLAUDE_CODE_USE_FOUNDRY: '',
    API_TIMEOUT_MS: '',
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '',
    ANTHROPIC_MODEL: '',
    ANTHROPIC_SMALL_FAST_MODEL: '',
    ANTHROPIC_DEFAULT_OPUS_MODEL: '',
    ANTHROPIC_DEFAULT_SONNET_MODEL: '',
    ANTHROPIC_DEFAULT_HAIKU_MODEL: '',
  }

  if (preset.authEnv === 'ANTHROPIC_API_KEY') {
    env.ANTHROPIC_AUTH_TOKEN = ''
    if (apiKey) env.ANTHROPIC_API_KEY = apiKey
  } else {
    env.ANTHROPIC_API_KEY = ''
    if (apiKey) env.ANTHROPIC_AUTH_TOKEN = apiKey
  }

  const baseUrl = getClaudeProviderBaseUrl(prefs)
  if (baseUrl) env.ANTHROPIC_BASE_URL = baseUrl

  if (preset.id === 'anthropic') {
    if (prefs.provider === 'bedrock') env.CLAUDE_CODE_USE_BEDROCK = '1'
    if (prefs.provider === 'vertex') env.CLAUDE_CODE_USE_VERTEX = '1'
    if (prefs.provider === 'foundry') env.CLAUDE_CODE_USE_FOUNDRY = '1'
  }

  if (preset.id === 'minimax') {
    const model = providerModelValue(prefs)
    env.API_TIMEOUT_MS = '3000000'
    env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC = '1'
    if (model) {
      env.ANTHROPIC_MODEL = model
      env.ANTHROPIC_SMALL_FAST_MODEL = model
      env.ANTHROPIC_DEFAULT_OPUS_MODEL = model
      env.ANTHROPIC_DEFAULT_SONNET_MODEL = model
      env.ANTHROPIC_DEFAULT_HAIKU_MODEL = model
    }
  }

  if (preset.id === 'zai') {
    env.API_TIMEOUT_MS = '3000000'
    const opus = providerOpusModelValue(prefs)
    const sonnet = providerSonnetModelValue(prefs)
    const haiku = providerHaikuModelValue(prefs)
    if (opus) env.ANTHROPIC_DEFAULT_OPUS_MODEL = opus
    if (sonnet) env.ANTHROPIC_DEFAULT_SONNET_MODEL = sonnet
    if (haiku) env.ANTHROPIC_DEFAULT_HAIKU_MODEL = haiku
  }

  mergeCustomEnv(env, prefs.customEnv)
  return env
}
