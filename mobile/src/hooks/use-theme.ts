import { Colors } from '@/constants/theme'
import { useColorScheme } from '@/hooks/use-color-scheme'

export function useTheme() {
  const scheme = useColorScheme()
  return Colors[scheme]
}
