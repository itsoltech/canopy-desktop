import type { WorktreeSetupAction } from '../db/types'

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isOptionalString(value: unknown): value is string | undefined {
  return value === undefined || typeof value === 'string'
}

function toAction(value: unknown): WorktreeSetupAction | null {
  if (!isRecord(value)) return null
  if (!isOptionalString(value.label)) return null

  if (value.type === 'command') {
    if (typeof value.command !== 'string') return null
    const action: WorktreeSetupAction = { type: 'command', command: value.command }
    if (value.label !== undefined) action.label = value.label
    return action
  }

  if (value.type === 'copy') {
    if (typeof value.source !== 'string') return null
    if (!isOptionalString(value.dest)) return null
    const action: WorktreeSetupAction = { type: 'copy', source: value.source }
    if (value.dest !== undefined) action.dest = value.dest
    if (value.label !== undefined) action.label = value.label
    return action
  }

  return null
}

/**
 * Parse the persisted `workspace:<id>:worktreeSetup` preference into setup actions.
 *
 * The stored value is not trustworthy: `SettingsExport` imports arbitrary
 * preference keys from a user-supplied file, and command actions are handed to a
 * shell by `WorktreeSetupRunner`. Validate the shape here instead of asserting
 * it, so a malformed or hostile payload is rejected before anything executes.
 *
 * Returns `null` when the JSON is invalid or any entry is not a well-formed action.
 */
export function parseWorktreeSetupActions(configJson: string): WorktreeSetupAction[] | null {
  let parsed: unknown
  try {
    parsed = JSON.parse(configJson)
  } catch {
    return null
  }

  if (!Array.isArray(parsed)) return null

  const actions: WorktreeSetupAction[] = []
  for (const entry of parsed) {
    const action = toAction(entry)
    if (!action) return null
    actions.push(action)
  }
  return actions
}
