import { ok, err } from '../errors'
import type { Result } from 'neverthrow'
import type { SkillError } from './errors'
import { isSkillAgentTarget, type SkillAgentTarget } from './types'

export interface ParsedSkill {
  name: string
  description: string
  version: string
  prompt: string
  agents: SkillAgentTarget[]
  metadata: Record<string, unknown>
}

/**
 * Parse YAML frontmatter from SKILL.md format: --- \n key: value \n --- \n body
 *
 * NOTE: This is a hand-rolled subset parser, not a full YAML implementation.
 * It intentionally handles only the cases found in skill files:
 *   - Simple scalar key-value pairs (string, boolean)
 *   - Inline arrays: `key: ['a', 'b']` (JSON-compatible syntax only)
 *   - Block sequence lists: `key:\n  - item`
 *
 * Limitations (not supported):
 *   - Nested mappings / multi-level objects
 *   - Quoted strings with escape sequences
 *   - Multi-line scalars (| or > block scalars)
 *   - Anchors, aliases, and merge keys
 *   - Numeric or null coercion beyond booleans
 *
 * If skill frontmatter becomes more complex, replace with the `yaml` npm package.
 */
function parseFrontmatter(content: string): { frontmatter: Record<string, unknown>; body: string } {
  const fmMatch = content.match(/^---\s*\n([\s\S]*?)\n---\s*\n([\s\S]*)$/)
  if (!fmMatch) return { frontmatter: {}, body: content.trim() }

  const rawYaml = fmMatch[1]
  const body = fmMatch[2].trim()
  const frontmatter: Record<string, unknown> = {}

  const lines = rawYaml.split('\n')
  let currentKey: string | null = null
  let currentList: string[] | null = null

  for (const line of lines) {
    // Multi-line YAML list item: "  - value"
    const listMatch = line.match(/^\s+-\s+(.+)$/)
    if (listMatch && currentKey) {
      if (!currentList) currentList = []
      currentList.push(listMatch[1].trim())
      continue
    }

    // Flush previous list
    if (currentKey && currentList) {
      frontmatter[currentKey] = currentList
      currentList = null
      currentKey = null
    }

    const colonIdx = line.indexOf(':')
    if (colonIdx === -1) continue
    const key = line.slice(0, colonIdx).trim()
    let value: unknown = line.slice(colonIdx + 1).trim()

    // Empty value after colon — start of a multi-line list
    if (value === '') {
      currentKey = key
      currentList = null
      continue
    }

    // Parse simple YAML arrays: ['a', 'b']
    if (typeof value === 'string' && value.startsWith('[') && value.endsWith(']')) {
      try {
        value = JSON.parse(value.replace(/'/g, '"'))
      } catch {
        // keep as string
      }
    }
    // Parse booleans
    if (value === 'true') value = true
    if (value === 'false') value = false

    if (key) frontmatter[key] = value
    currentKey = null
  }

  // Flush trailing list
  if (currentKey && currentList) {
    frontmatter[currentKey] = currentList
  }

  return { frontmatter, body }
}

/** Detect format and parse skill content */
export function parseSkillContent(
  content: string,
  sourceUri: string,
  fileName?: string,
): Result<ParsedSkill, SkillError> {
  const trimmed = content.trim()
  if (!trimmed) {
    return err({ _tag: 'ParseFailed', source: sourceUri, reason: 'Empty content' })
  }

  // Detect SKILL.md format (YAML frontmatter + markdown)
  if (trimmed.startsWith('---')) {
    return parseSkillMd(trimmed, sourceUri, fileName)
  }

  // Raw markdown — treat as prompt-only, user must specify agents
  return parseRawMarkdown(trimmed, sourceUri, fileName)
}

function parseSkillMd(
  content: string,
  sourceUri: string,
  fileName?: string,
): Result<ParsedSkill, SkillError> {
  const { frontmatter, body } = parseFrontmatter(content)

  if (!body) {
    return err({
      _tag: 'ParseFailed',
      source: sourceUri,
      reason: 'No prompt body after frontmatter',
    })
  }

  // Frontmatter values are untrusted: the hand-rolled parser can yield arrays
  // (inline `[a, b]` or block lists) or booleans. Guard the type instead of
  // asserting it — a non-string name would otherwise crash `.toLowerCase()` in
  // the installer, and a non-array `agents` would be iterated char-by-char.
  const name =
    typeof frontmatter.name === 'string' ? frontmatter.name : (fileName ?? 'unnamed-skill')
  const description = typeof frontmatter.description === 'string' ? frontmatter.description : ''
  const version = typeof frontmatter.version === 'string' ? frontmatter.version : '1.0.0'
  const rawAgents = frontmatter.agents
  // Checking "is a string" is not the same as "is a known agent target": an
  // unrecognised entry such as `vscode` would otherwise be carried as a valid
  // target and silently no-op in SkillTransformer. Reuse the canonical guard
  // and drop anything outside the allow-list.
  const agents: SkillAgentTarget[] = Array.isArray(rawAgents)
    ? rawAgents.filter((a): a is SkillAgentTarget => typeof a === 'string' && isSkillAgentTarget(a))
    : ['claude']
  const metadata: Record<string, unknown> = {}

  if (frontmatter['allowed-tools']) metadata['allowed-tools'] = frontmatter['allowed-tools']
  if (frontmatter['disable-model-invocation'])
    metadata['disable-model-invocation'] = frontmatter['disable-model-invocation']

  return ok({ name, description, version, prompt: body, agents, metadata })
}

function parseRawMarkdown(
  content: string,
  _sourceUri: string,
  fileName?: string,
): Result<ParsedSkill, SkillError> {
  const name = fileName ?? 'unnamed-skill'
  return ok({
    name,
    description: '',
    version: '1.0.0',
    prompt: content,
    agents: [],
    metadata: {},
  })
}
