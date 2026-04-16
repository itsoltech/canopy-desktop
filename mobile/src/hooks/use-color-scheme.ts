import { useColorScheme as useRNColorScheme } from 'react-native'

import { useAppPreferences } from '@/hooks/use-app-preferences'

export function useColorScheme(): 'light' | 'dark' {
  const os = useRNColorScheme() ?? 'light'
  const { appTheme } = useAppPreferences()
  if (appTheme === 'system') return os === 'dark' ? 'dark' : 'light'
  return appTheme
}

/** Resolves the effective color scheme for the terminal.
 *  When `terminalThemeMode` is 'system' the terminal follows the resolved app
 *  color scheme (itself already OS-aware). When set explicitly to 'light' or
 *  'dark', the terminal uses that value regardless of the app theme. */
export function useTerminalColorScheme(): 'light' | 'dark' {
  const appScheme = useColorScheme()
  const { terminalThemeMode } = useAppPreferences()
  if (terminalThemeMode === 'system') return appScheme
  return terminalThemeMode
}
