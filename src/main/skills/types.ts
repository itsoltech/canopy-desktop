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
