import type { CanopySkill, SkillListOptions } from './types'
import type { SkillStore } from './SkillStore'

export class SkillRegistry {
  private skills = new Map<string, CanopySkill>()

  constructor(private store: SkillStore) {
    this.reload()
  }

  private reload(): void {
    this.skills.clear()
    for (const skill of this.store.getAll()) {
      this.skills.set(skill.id, skill)
    }
  }

  getAll(): CanopySkill[] {
    return [...this.skills.values()]
  }

  get(id: string): CanopySkill | undefined {
    return this.skills.get(id)
  }

  list(opts?: SkillListOptions): CanopySkill[] {
    return this.store.list(opts)
  }

  refresh(): void {
    this.reload()
  }
}
