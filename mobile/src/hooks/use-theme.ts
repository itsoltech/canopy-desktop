import { Colors } from '@/constants/theme'
import { useColorScheme } from '@/hooks/use-color-scheme'

export type AppTheme = (typeof Colors)['light'] | (typeof Colors)['dark']

export function useTheme(): AppTheme {
  const scheme = useColorScheme()
  return Colors[scheme]
}
