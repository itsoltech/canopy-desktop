import { useColorScheme as useRNColorScheme } from 'react-native'

import { useAppPreferences } from '@/hooks/use-app-preferences'

export function useColorScheme(): 'light' | 'dark' {
  const os = useRNColorScheme() ?? 'light'
  const { appTheme } = useAppPreferences()
  if (appTheme === 'system') return os === 'dark' ? 'dark' : 'light'
  return appTheme
}
