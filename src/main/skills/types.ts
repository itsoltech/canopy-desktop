import type { AgentType } from '../agents/types'
import { KNOWN_AGENT_TYPES } from '../profiles/types'

export type SkillAgentTarget = AgentType | 'cursor' | 'opencode'

/**
 * Runtime allow-list of valid skill-agent targets, used to validate untrusted
 * IPC input before it is persisted into a skill's `enabledAgents`. Derived from
 * the canonical agent-type list (so new agents stay in sync automatically) plus
 * the skill-only `cursor` target.
 */
export const SKILL_AGENT_TARGETS: readonly SkillAgentTarget[] = [...KNOWN_AGENT_TYPES, 'cursor']

export function isSkillAgentTarget(value: string): value is SkillAgentTarget {
  return SKILL_AGENT_TARGETS.some((target) => target === value)
}

/**
 * Validates `skills:install` input from the untrusted renderer before any of it is persisted.
 * Returns the rejection reason, or null when the options are well-formed. (An unknown scope,
 * for instance, used to skip deployment while the skill was still recorded as installed.)
 */
export function skillInstallOptionsError(opts: unknown): string | null {
  if (typeof opts !== 'object' || opts === null) return 'Invalid install options'
  const { source, agents, scope, method, workspaceId, workspacePath } = opts as Record<
    string,
    unknown
  >
  if (typeof source !== 'string' || source.trim() === '') return 'Invalid skill source'
  if (
    agents !== undefined &&
    (!Array.isArray(agents) ||
      !agents.every((agent) => typeof agent === 'string' && isSkillAgentTarget(agent)))
  ) {
    return 'Invalid agent target'
  }
  if (scope !== undefined && scope !== 'project' && scope !== 'global') {
    return 'Invalid skill scope'
  }
  if (method !== undefined && method !== 'copy' && method !== 'symlink') {
    return 'Invalid install method'
  }
  if (workspaceId !== undefined && workspaceId !== null && typeof workspaceId !== 'string') {
    return 'Invalid workspace id'
  }
  if (workspacePath !== undefined && workspacePath !== null && typeof workspacePath !== 'string') {
    return 'Invalid workspace path'
  }
  return null
}

export type SkillSourceType = 'github' | 'url' | 'local'

export type SkillInstallMethod = 'copy' | 'symlink'

export type SkillScope = 'project' | 'global'

export interface CanopySkill {
  id: string
  name: string
  description: string
  version: string
  prompt: string
  agents: SkillAgentTarget[]
  metadata: Record<string, unknown>
  sourceType: SkillSourceType
  sourceUri: string
  installMethod: SkillInstallMethod
  scope: SkillScope
  workspaceId: string | null
  enabledAgents: SkillAgentTarget[]
  installedAt: string
}

export interface SkillInstallOptions {
  source: string
  agents?: SkillAgentTarget[]
  scope?: SkillScope
  method?: SkillInstallMethod
  workspaceId?: string | null
  workspacePath?: string
}

export interface SkillListOptions {
  scope?: SkillScope
  agent?: SkillAgentTarget
  workspaceId?: string | null
}
