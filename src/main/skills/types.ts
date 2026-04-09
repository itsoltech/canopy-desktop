import type { AgentType } from '../agents/types'

export type SkillAgentTarget = AgentType | 'cursor' | 'opencode'

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
