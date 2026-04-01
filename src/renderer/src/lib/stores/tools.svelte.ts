interface ToolDefinition {
  id: string
  name: string
  command: string
  args: string[]
  icon: string
  category: string
  isCustom: boolean
}

// --- State ---

let tools: ToolDefinition[] = $state([])
let availability: Record<string, boolean> = $state({})
let initialized = false
let unsubscribe: (() => void) | null = null

// --- Accessors ---

export function getTools(): ToolDefinition[] {
  return tools
}

export function getToolAvailability(): Record<string, boolean> {
  return availability
}

// --- Init ---

export async function initToolStore(): Promise<void> {
  if (initialized) return
  initialized = true

  const [fetchedTools, fetchedAvailability] = await Promise.all([
    window.api.listTools(),
    window.api.checkToolAvailability(),
  ])

  tools = fetchedTools
  availability = fetchedAvailability

  let availabilityGen = 0

  unsubscribe = window.api.onToolsChanged(async (updated) => {
    tools = updated
    const gen = ++availabilityGen
    const avail = await window.api.checkToolAvailability()
    if (gen === availabilityGen) availability = avail
  })
}

// --- Cleanup ---

export function destroyToolStore(): void {
  unsubscribe?.()
  unsubscribe = null
  initialized = false
  tools = []
  availability = {}
}

// --- Refresh ---

export async function refreshAvailability(): Promise<void> {
  availability = await window.api.checkToolAvailability()
}
