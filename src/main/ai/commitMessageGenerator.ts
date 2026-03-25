import { execFile } from 'child_process'
import os from 'os'
import { query } from '@anthropic-ai/claude-agent-sdk'
import type { PreferencesStore } from '../db/PreferencesStore'
import { getLoginEnv } from '../shell/loginEnv'
import { BLOCKED_ENV_VARS } from '../security/envBlocklist'

let cachedClaudePath: string | undefined

async function resolveClaudeExecutable(): Promise<string | undefined> {
  if (cachedClaudePath !== undefined) return cachedClaudePath || undefined
  const cmd = os.platform() === 'win32' ? 'where' : 'which'
  const env = getLoginEnv() ?? (process.env as Record<string, string>)
  return new Promise((resolve) => {
    execFile(cmd, ['claude'], { env }, (err, stdout) => {
      cachedClaudePath = err ? '' : stdout.trim()
      resolve(cachedClaudePath || undefined)
    })
  })
}

const MAX_DIFF_LENGTH = 15_000

const PROMPT_TEMPLATE = `Generate a concise git commit message for this diff.
Use conventional commits format (feat:, fix:, chore:, refactor:, docs:, test:, build:).
First line (subject) under 72 chars. Add a body only if the change is complex.

<diff>
{diff}
</diff>`

interface CommitOutput {
  subject?: string
  body?: string
}

const OUTPUT_SCHEMA = {
  type: 'object' as const,
  properties: {
    subject: {
      type: 'string' as const,
      description: 'Commit subject line in conventional commits format, under 72 chars',
    },
    body: {
      type: 'string' as const,
      description: 'Optional commit body with details. Empty string if not needed.',
    },
  },
  required: ['subject', 'body'] as const,
}

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
      const parsed = JSON.parse(claudeCustomEnv)
      for (const [k, v] of Object.entries(parsed)) {
        if (!BLOCKED_ENV_VARS.has(k) && typeof v === 'string') {
          envOverrides[k] = v
        }
      }
    } catch {
      // Invalid JSON
    }
  }

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
    const claudePath = await resolveClaudeExecutable()

    const q = query({
      prompt,
      options: {
        model,
        pathToClaudeCodeExecutable: claudePath,
        outputFormat: { type: 'json_schema', schema: OUTPUT_SCHEMA },
      },
    })

    let structuredOutput: CommitOutput | null = null
    for await (const message of q) {
      if (message.type === 'result' && (message as { subtype?: string }).subtype === 'success') {
        structuredOutput = (message as Record<string, unknown>)
          .structured_output as CommitOutput | null
      }
    }

    if (!structuredOutput?.subject) return null
    const { subject, body } = structuredOutput
    return body ? `${subject}\n\n${body}` : subject
  } catch (err) {
    console.error('[ai-commit] Error:', err)
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
