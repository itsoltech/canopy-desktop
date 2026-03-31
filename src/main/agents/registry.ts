import type { AgentAdapter } from './types'

const adapters = new Map<string, AgentAdapter>()

export function registerAdapter(adapter: AgentAdapter): void {
  adapters.set(adapter.toolId, adapter)
}

export function getAdapter(toolId: string): AgentAdapter | undefined {
  return adapters.get(toolId)
}

export function isAgentTool(toolId: string): boolean {
  return adapters.has(toolId)
}

export function allAdapters(): AgentAdapter[] {
  return [...adapters.values()]
}
