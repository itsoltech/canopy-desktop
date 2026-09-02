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

  try {
    skills = await window.api.listSkills()
  } catch (e) {
    // Unlatch so a later caller can retry. Leaving `initialized` set would pin
    // the store to an empty list for the rest of the session, which the UI
    // renders as "no skills installed" — indistinguishable from a real failure.
    initialized = false
    console.error('[skills] listSkills failed:', e)
    return
  }

  unsubscribe = window.api.onSkillsChanged((updated) => {
    // The skills:changed IPC payload is produced from the typed SkillStore in the
    // main process; the preload callback surfaces it untyped, so assert the shape here.
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
