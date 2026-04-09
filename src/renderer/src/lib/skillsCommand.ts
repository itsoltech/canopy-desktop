export interface ParsedSkillCommand {
  action: 'install' | 'list' | 'remove' | 'update' | 'info'
  args: Record<string, string | boolean | string[]>
  positional: string[]
}

export function isSkillsCommand(input: string): boolean {
  return input.trim().startsWith('canopy skills ')
}

export function parseSkillsCommand(input: string): ParsedSkillCommand | null {
  const tokens = input.trim().split(/\s+/)
  if (tokens.length < 3 || tokens[0] !== 'canopy' || tokens[1] !== 'skills') return null

  const action = tokens[2] as ParsedSkillCommand['action']
  if (!['install', 'list', 'remove', 'update', 'info'].includes(action)) return null

  const args: Record<string, string | boolean | string[]> = {}
  const positional: string[] = []

  let i = 3
  while (i < tokens.length) {
    const token = tokens[i]
    if (token.startsWith('--')) {
      const key = token.slice(2)
      const next = tokens[i + 1]
      if (next && !next.startsWith('--')) {
        args[key] = next
        i += 2
      } else {
        args[key] = true
        i += 1
      }
    } else {
      positional.push(token)
      i += 1
    }
  }

  return { action, args, positional }
}
