import { randomUUID } from 'crypto'

// Pure helpers for tmux session naming. Extracted so modules that only need
// naming logic (e.g. the IPC tool-spawn Effect program) can import without
// pulling in the full TmuxManager graph (which transitively requires
// @electron-toolkit/utils and Electron app state).

export function tmuxSessionName(workspaceId: string, suffix?: string): string {
  const wsPrefix = workspaceId.slice(0, 12)
  const id = suffix ?? randomUUID().slice(0, 8)
  return `canopy-${wsPrefix}-${id}`
}

export function isCanopyTmuxSession(name: string): boolean {
  return name.startsWith('canopy-')
}

export function workspaceIdFromTmuxSession(name: string): string | null {
  const match = name.match(/^canopy-([a-f0-9-]{1,12})-/)
  return match ? match[1] : null
}
