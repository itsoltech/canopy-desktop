interface SkillDefinition {
  id: string
  name: string
  description: string
  version: string
  prompt: string
  agents: string[]
  metadata: Record<string, unknown>
  sourceType: string
  sourceUri: string
  installMethod: string
  scope: string
  workspaceId: string | null
  enabledAgents: string[]
  installedAt: string
}

// --- State ---

let skills: SkillDefinition[] = $state([])
let initialized = false
let unsubscribe: (() => void) | null = null

// --- Accessors ---

export function getSkills(): SkillDefinition[] {
  return skills
}

export function getSkillsByAgent(agent: string): SkillDefinition[] {
  return skills.filter((s) => s.agents.includes(agent))
}

// --- Init ---

export async function initSkillStore(): Promise<void> {
  if (initialized) return
  initialized = true

  skills = await window.api.listSkills()

  unsubscribe = window.api.onSkillsChanged((updated) => {
    skills = updated as SkillDefinition[]
  })
}

// --- Cleanup ---

export function destroySkillStore(): void {
  unsubscribe?.()
  unsubscribe = null
  initialized = false
  skills = []
}
