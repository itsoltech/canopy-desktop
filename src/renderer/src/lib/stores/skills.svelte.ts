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
let disposed = false
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
  disposed = false

  const fetched = await window.api.listSkills()

  // destroySkillStore() can run while the fetch above is in flight. Its
  // `unsubscribe?.()` is a no-op that early because the handle below does not
  // exist yet, so subscribing now would strand a listener nothing can remove —
  // and re-init would stack another on top of it.
  if (disposed) return

  skills = fetched

  unsubscribe = window.api.onSkillsChanged((updated) => {
    // The skills:changed IPC payload is produced from the typed SkillStore in the
    // main process; the preload callback surfaces it untyped, so assert the shape here.
    skills = updated as SkillDefinition[]
  })
}

// --- Cleanup ---

export function destroySkillStore(): void {
  disposed = true
  unsubscribe?.()
  unsubscribe = null
  initialized = false
  skills = []
}
