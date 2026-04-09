import { ok, err } from '../errors'
import type { Result } from 'neverthrow'
import type { SkillError } from './errors'
import type { SkillAgentTarget } from './types'

export interface ParsedSkill {
  name: string
  description: string
  version: string
  prompt: string
  agents: SkillAgentTarget[]
  metadata: Record<string, unknown>
}

/** Parse YAML frontmatter from SKILL.md format: --- \n key: value \n --- \n body */
function parseFrontmatter(content: string): { frontmatter: Record<string, unknown>; body: string } {
  const fmMatch = content.match(/^---\s*\n([\s\S]*?)\n---\s*\n([\s\S]*)$/)
  if (!fmMatch) return { frontmatter: {}, body: content.trim() }

  const rawYaml = fmMatch[1]
  const body = fmMatch[2].trim()
  const frontmatter: Record<string, unknown> = {}

  for (const line of rawYaml.split('\n')) {
    const colonIdx = line.indexOf(':')
    if (colonIdx === -1) continue
    const key = line.slice(0, colonIdx).trim()
    let value: unknown = line.slice(colonIdx + 1).trim()

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

  const name = (frontmatter.name as string) ?? fileName ?? 'unnamed-skill'
  const description = (frontmatter.description as string) ?? ''
  const version = (frontmatter.version as string) ?? '1.0.0'
  const agents = (frontmatter.agents as SkillAgentTarget[]) ?? ['claude']
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
