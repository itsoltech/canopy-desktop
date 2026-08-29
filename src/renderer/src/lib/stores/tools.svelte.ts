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
// True once the initial tool list + availability have loaded. Consumers use
// this to tell "still loading" apart from a genuinely empty result, so they
// don't flash an empty-state message during the initial async fetch.
let ready = $state(false)
let initialized = false
let disposed = false
let unsubscribe: (() => void) | null = null

// --- Accessors ---

export function getTools(): ToolDefinition[] {
  return tools
}

export function getToolAvailability(): Record<string, boolean> {
  return availability
}

export function getToolsReady(): boolean {
  return ready
}

// --- Init ---

export async function initToolStore(): Promise<void> {
  if (initialized) return
  initialized = true
  disposed = false

  const [fetchedTools, fetchedAvailability] = await Promise.all([
    window.api.listTools(),
    window.api.checkToolAvailability(),
  ])

  // destroyToolStore() can run while the fetch above is in flight. Its
  // `unsubscribe?.()` is a no-op that early because the handle below does not
  // exist yet, so subscribing now would strand a listener nothing can remove —
  // and re-init would stack another on top of it.
  if (disposed) return

  tools = fetchedTools
  availability = fetchedAvailability
  ready = true

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
  disposed = true
  unsubscribe?.()
  unsubscribe = null
  initialized = false
  ready = false
  tools = []
  availability = {}
}

// --- Refresh ---

export async function refreshAvailability(): Promise<void> {
  availability = await window.api.checkToolAvailability()
}
