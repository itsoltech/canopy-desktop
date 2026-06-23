import { Stack } from 'expo-router'

import { Colors } from '@/constants/theme'
import { useColorScheme } from '@/hooks/use-color-scheme'

export default function HomeStackLayout(): React.ReactElement {
  const scheme = useColorScheme()
  return (
    <Stack
      screenOptions={{
        headerLargeTitle: true,
        contentStyle: { backgroundColor: Colors[scheme].background },
      }}
    >
      <Stack.Screen name="index" options={{ title: 'Instances' }} />
    </Stack>
  )
}
