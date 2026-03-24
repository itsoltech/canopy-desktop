import { query } from '@anthropic-ai/claude-agent-sdk'
import type { PreferencesStore } from '../db/PreferencesStore'

const MAX_DIFF_LENGTH = 15_000

const PROMPT_TEMPLATE = `Generate a concise git commit message for this diff.
Use conventional commits format (feat:, fix:, chore:, refactor:, docs:, test:, build:).
First line under 72 chars. Add a body only if the change is complex.
Output ONLY the commit message, no backticks or quotes.

<diff>
{diff}
</diff>`

export async function generateCommitMessage(
  diff: string,
  preferencesStore: PreferencesStore,
): Promise<string | null> {
  // Build env overrides from preferences (same pattern as handlers.ts for PTY sessions)
  const envOverrides: Record<string, string> = {}

  const claudeApiKey = preferencesStore.get('claude.apiKey')
  const claudeBaseUrl = preferencesStore.get('claude.baseUrl')
  const claudeProvider = preferencesStore.get('claude.provider')
  const claudeCustomEnv = preferencesStore.get('claude.customEnv')

  if (claudeApiKey) envOverrides.ANTHROPIC_API_KEY = claudeApiKey
  if (claudeBaseUrl) envOverrides.ANTHROPIC_BASE_URL = claudeBaseUrl
  if (claudeProvider === 'bedrock') envOverrides.CLAUDE_CODE_USE_BEDROCK = '1'
  if (claudeProvider === 'vertex') envOverrides.CLAUDE_CODE_USE_VERTEX = '1'
  if (claudeProvider === 'foundry') envOverrides.CLAUDE_CODE_USE_FOUNDRY = '1'
  if (claudeCustomEnv) {
    try {
      Object.assign(envOverrides, JSON.parse(claudeCustomEnv))
    } catch {
      // Invalid JSON
    }
  }

  // Must have an API key from either preferences or environment
  if (!envOverrides.ANTHROPIC_API_KEY && !process.env.ANTHROPIC_API_KEY) return null

  const model = 'haiku'

  // Save current env, apply overrides, restore in finally
  const savedEnv: Record<string, string | undefined> = {}

  try {
    for (const [key, val] of Object.entries(envOverrides)) {
      savedEnv[key] = process.env[key]
      process.env[key] = val
    }

    const truncatedDiff = diff.length > MAX_DIFF_LENGTH ? diff.slice(0, MAX_DIFF_LENGTH) : diff
    const prompt = PROMPT_TEMPLATE.replace('{diff}', truncatedDiff)

    const q = query({ prompt, options: { model } })

    let result = ''
    for await (const message of q) {
      if (message.type === 'assistant') {
        const content = (message as { message?: { content?: unknown[] } }).message?.content
        if (Array.isArray(content)) {
          for (const block of content) {
            if (block && typeof block === 'object' && 'text' in block) {
              result += (block as { text: string }).text
            }
          }
        }
      }
    }

    return cleanMessage(result)
  } catch {
    return null
  } finally {
    for (const [key, val] of Object.entries(savedEnv)) {
      if (val !== undefined) {
        process.env[key] = val
      } else {
        delete process.env[key]
      }
    }
  }
}

function cleanMessage(raw: string): string | null {
  let msg = raw.trim()
  // Strip markdown fences
  msg = msg.replace(/^```[\s\S]*?\n/, '').replace(/\n```\s*$/, '')
  // Strip surrounding quotes
  msg = msg.replace(/^["']|["']$/g, '')
  msg = msg.trim()
  return msg || null
}
