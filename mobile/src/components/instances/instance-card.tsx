import { SymbolView } from 'expo-symbols'
import { Pressable, StyleSheet, View } from 'react-native'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'
import type { SavedInstance } from '@/lib/storage/saved-instances-types'

type InstanceCardProps = {
  instance: SavedInstance
  onPress: () => void
}

export function InstanceCard({ instance, onPress }: InstanceCardProps): React.ReactElement {
  const theme = useTheme()

  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [styles.pressable, pressed && styles.pressed]}
    >
      <ThemedView type="backgroundElement" style={styles.card}>
        <View style={styles.headerRow}>
          <ThemedText type="smallBold" numberOfLines={1} style={styles.nickname}>
            {instance.nickname}
          </ThemedText>
          <SymbolView
            name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
            size={14}
            weight="medium"
            tintColor={theme.textSecondary}
          />
        </View>
        <ThemedText type="small" themeColor="textSecondary" numberOfLines={1}>
          {instance.hostname}
        </ThemedText>
        <ThemedText type="code" themeColor="textSecondary" numberOfLines={1}>
          {instance.lanIp}:{instance.port}
        </ThemedText>
        <ThemedText type="small" themeColor="textSecondary" style={styles.lastConnected}>
          {formatLastConnected(instance.lastConnectedAt)}
        </ThemedText>
      </ThemedView>
    </Pressable>
  )
}

function formatLastConnected(iso?: string): string {
  if (!iso) return 'Never connected'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return 'Never connected'
  const diffMs = Date.now() - d.getTime()
  const mins = Math.floor(diffMs / 60_000)
  if (mins < 1) return 'Last connected just now'
  if (mins < 60) return `Last connected ${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `Last connected ${hours}h ago`
  const days = Math.floor(hours / 24)
  return `Last connected ${days}d ago`
}

const styles = StyleSheet.create({
  pressable: {
    borderRadius: Spacing.three,
  },
  pressed: {
    opacity: 0.7,
  },
  card: {
    borderRadius: Spacing.three,
    padding: Spacing.three,
    gap: Spacing.half,
  },
  headerRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: Spacing.two,
  },
  nickname: {
    flex: 1,
  },
  lastConnected: {
    marginTop: Spacing.one,
  },
})
