import { SymbolView } from 'expo-symbols'
import { Pressable, StyleSheet } from 'react-native'

import { BottomTabInset, Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'

type AddInstanceFabProps = {
  onPress: () => void
}

export function AddInstanceFab({ onPress }: AddInstanceFabProps): React.ReactElement {
  const theme = useTheme()

  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        styles.fab,
        { backgroundColor: theme.text },
        pressed && styles.pressed,
      ]}
      accessibilityLabel="Add instance"
      accessibilityRole="button"
    >
      <SymbolView
        name={{ ios: 'plus', android: 'add', web: 'add' }}
        size={24}
        weight="semibold"
        tintColor={theme.background}
      />
    </Pressable>
  )
}

const styles = StyleSheet.create({
  fab: {
    position: 'absolute',
    right: Spacing.four,
    bottom: BottomTabInset + Spacing.four,
    width: 56,
    height: 56,
    borderRadius: 28,
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.2,
    shadowRadius: 8,
    elevation: 6,
  },
  pressed: {
    opacity: 0.85,
    transform: [{ scale: 0.96 }],
  },
})
