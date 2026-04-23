import type { PermissionMode } from '../../../../main/sdkAgents/types'

export interface PermissionModeOption {
  value: PermissionMode
  label: string
}

export const MODE_OPTIONS: readonly PermissionModeOption[] = [
  { value: 'default', label: 'Default' },
  { value: 'plan', label: 'Plan' },
  { value: 'acceptEdits', label: 'Auto-accept edits' },
  { value: 'bypassPermissions', label: 'Bypass permissions' },
]

export function cyclePermissionMode(current: PermissionMode): PermissionMode {
  const idx = MODE_OPTIONS.findIndex((m) => m.value === current)
  const next = (idx + 1) % MODE_OPTIONS.length
  return MODE_OPTIONS[next]!.value
}

/**
 * Readline-style history navigator backed by an immutable list of past prompts
 * (newest first). Stashes an in-progress draft on the first `up()` so the user
 * can return to it by pressing `down()` past the newest entry.
 */
export interface HistoryNavigator {
  up(currentDraft: string): string | null
  down(): string | null
  reset(): void
  readonly active: boolean
}

export function makeHistoryNavigator(history: readonly string[]): HistoryNavigator {
  let index = -1
  let stashedDraft = ''

  return {
    up(currentDraft: string): string | null {
      if (history.length === 0) return null
      if (index === -1) stashedDraft = currentDraft
      if (index < history.length - 1) index += 1
      return history[index] ?? null
    },
    down(): string | null {
      if (index === -1) return null
      if (index === 0) {
        index = -1
        const draft = stashedDraft
        stashedDraft = ''
        return draft
      }
      index -= 1
      return history[index] ?? null
    },
    reset(): void {
      index = -1
      stashedDraft = ''
    },
    get active(): boolean {
      return index !== -1
    },
  }
}

export interface KeybindingDoc {
  keys: string
  description: string
  context?: string
}

/** Single source-of-truth table — rendered in ShortcutsOverlay and ShortcutsPrefs. */
export const KEYBINDING_DOCS: readonly KeybindingDoc[] = [
  { keys: 'Enter', description: 'Send message' },
  { keys: 'Shift+Enter · Ctrl+J', description: 'Insert newline' },
  { keys: '⌘⏎ / Ctrl+Enter', description: 'Force send (works from any line)' },
  { keys: '⌘. / Ctrl+. · Esc', description: 'Cancel current turn', context: 'while streaming' },
  { keys: '↑', description: 'Recall previous prompt', context: 'input empty' },
  { keys: '↓', description: 'Move forward through history', context: 'after ↑' },
  { keys: 'Shift+Tab', description: 'Cycle permission mode' },
  { keys: '⌘L / Ctrl+L', description: 'Clear input' },
  { keys: 'Alt+P', description: 'Open model picker' },
  { keys: 'Alt+T', description: 'Open effort picker', context: 'reasoning models only' },
  { keys: '@', description: 'File path autocomplete', context: 'in input' },
  { keys: '/', description: 'Slash command menu', context: 'at start of input' },
  { keys: '⌘R / Ctrl+R', description: 'Search past prompts' },
  { keys: 'Ctrl+O', description: 'Toggle expand-all tool/thinking blocks' },
  { keys: '?', description: 'Show this cheat sheet', context: 'input empty' },
]
