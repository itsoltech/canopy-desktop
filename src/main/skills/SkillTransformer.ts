import type { Result } from 'neverthrow'
import type { SkillError } from './errors'
import type { CanopySkill, SkillAgentTarget } from './types'

export interface SkillTransformer {
  readonly agent: SkillAgentTarget
  readonly projectDir: string
  globalDir(): string
  deploy(skill: CanopySkill, targetRoot: string): Promise<Result<string, SkillError>>
  undeploy(skill: CanopySkill, targetRoot: string): Promise<Result<void, SkillError>>
}

const transformers = new Map<SkillAgentTarget, SkillTransformer>()

export function registerTransformer(t: SkillTransformer): void {
  transformers.set(t.agent, t)
}

export function getTransformer(agent: SkillAgentTarget): SkillTransformer | undefined {
  return transformers.get(agent)
}

export function allTransformers(): SkillTransformer[] {
  return [...transformers.values()]
}
