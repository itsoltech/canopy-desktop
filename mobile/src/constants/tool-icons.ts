import { SymbolView } from 'expo-symbols'

type IconDescriptor = Parameters<typeof SymbolView>[0]['name']

export function resolveToolIcon(toolId: string): IconDescriptor {
  switch (toolId) {
    case 'claude':
      return { ios: 'sparkles', android: 'auto_awesome', web: 'auto_awesome' }
    case 'codex':
      return { ios: 'wand.and.stars', android: 'auto_fix_high', web: 'auto_fix_high' }
    case 'gemini':
      return { ios: 'sparkle', android: 'auto_awesome', web: 'auto_awesome' }
    case 'opencode':
      return {
        ios: 'chevron.left.forwardslash.chevron.right',
        android: 'code',
        web: 'code',
      }
    case 'browser':
      return { ios: 'globe', android: 'public', web: 'public' }
    case 'shell':
    default:
      return { ios: 'terminal', android: 'terminal', web: 'terminal' }
  }
}
