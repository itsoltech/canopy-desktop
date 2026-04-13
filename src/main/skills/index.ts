import type { Database } from '../db/Database'
import { SkillStore } from './SkillStore'
import { SkillRegistry } from './SkillRegistry'
import { SkillInstaller } from './SkillInstaller'
import { registerTransformer } from './SkillTransformer'
import { claudeTransformer } from './transformers/claude'
import { geminiTransformer } from './transformers/gemini'
import { cursorTransformer } from './transformers/cursor'
import { opencodeTransformer } from './transformers/opencode'

export function initSkills(database: Database): {
  store: SkillStore
  registry: SkillRegistry
  installer: SkillInstaller
} {
  registerTransformer(claudeTransformer)
  registerTransformer(geminiTransformer)
  registerTransformer(cursorTransformer)
  registerTransformer(opencodeTransformer)

  const store = new SkillStore(database)
  const registry = new SkillRegistry(store)
  const installer = new SkillInstaller(store)

  return { store, registry, installer }
}

export { SkillStore } from './SkillStore'
export { SkillRegistry } from './SkillRegistry'
export { SkillInstaller } from './SkillInstaller'
export type { CanopySkill, SkillInstallOptions, SkillListOptions } from './types'
