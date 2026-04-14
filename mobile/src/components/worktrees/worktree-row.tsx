import { SymbolView } from 'expo-symbols'
import { Pressable, StyleSheet, View } from 'react-native'

import { ThemedText } from '@/components/themed-text'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'
import type { WorktreeSnapshot } from '@/lib/mock/snapshot-types'

type WorktreeRowProps = {
  worktree: WorktreeSnapshot
  onPress: () => void
}

export function WorktreeRow({ worktree, onPress }: WorktreeRowProps): React.ReactElement {
  const theme = useTheme()

  return (
    <Pressable onPress={onPress} style={({ pressed }) => [styles.row, pressed && styles.pressed]}>
      <SymbolView
        name={
          worktree.isMain
            ? { ios: 'circle.fill', android: 'circle', web: 'circle' }
            : { ios: 'circle', android: 'radio_button_unchecked', web: 'radio_button_unchecked' }
        }
        size={12}
        weight="semibold"
        tintColor={worktree.isMain ? theme.text : theme.textSecondary}
      />
      <View style={styles.label}>
        <ThemedText type="small" numberOfLines={1}>
          {worktree.branch}
        </ThemedText>
        {worktree.isMain && (
          <ThemedText type="small" themeColor="textSecondary">
            main
          </ThemedText>
        )}
      </View>
      <SymbolView
        name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
        size={12}
        weight="medium"
        tintColor={theme.textSecondary}
      />
    </Pressable>
  )
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.three,
  },
  pressed: {
    opacity: 0.5,
  },
  label: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
  },
})
