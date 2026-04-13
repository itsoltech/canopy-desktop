import type { AgentProfileMasked, ProfileInput } from '../../../../main/profiles/types'
import type { AgentType } from '../../../../main/agents/types'

let profiles: AgentProfileMasked[] = $state([])
let initialized = false
let unsubscribe: (() => void) | null = null

export function getProfiles(): AgentProfileMasked[] {
  return profiles
}

export function getProfilesByAgent(agentType: AgentType): AgentProfileMasked[] {
  return profiles
    .filter((p) => p.agentType === agentType)
    .sort((a, b) => a.sortIndex - b.sortIndex || a.name.localeCompare(b.name))
}

export function getProfileById(id: string): AgentProfileMasked | undefined {
  return profiles.find((p) => p.id === id)
}

export async function initProfileStore(): Promise<void> {
  if (initialized) return
  initialized = true
  try {
    profiles = await window.api.listProfiles()
  } catch (e) {
    console.warn('Failed to load profiles:', e)
    profiles = []
  }
  unsubscribe = window.api.onProfilesChanged((list) => {
    profiles = list
  })
}

export async function saveProfile(input: ProfileInput): Promise<AgentProfileMasked> {
  const saved = await window.api.saveProfile(input)
  // The onProfilesChanged broadcast will deliver the full list shortly;
  // also patch optimistically for a snappy UI.
  const idx = profiles.findIndex((p) => p.id === saved.id)
  if (idx >= 0) {
    profiles[idx] = saved
  } else {
    profiles = [...profiles, saved]
  }
  return saved
}

export async function deleteProfile(id: string): Promise<void> {
  await window.api.deleteProfile(id)
  profiles = profiles.filter((p) => p.id !== id)
}

export function destroyProfileStore(): void {
  unsubscribe?.()
  unsubscribe = null
  initialized = false
  profiles = []
}
