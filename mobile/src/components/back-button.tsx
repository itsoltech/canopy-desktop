import { useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { Pressable, StyleSheet, type StyleProp, type ViewStyle } from 'react-native'

import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'

const BACK_ICON = { ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' } as const

type BackButtonProps = {
  /** Defaults to `router.back()`. */
  onPress?: () => void
  accessibilityLabel?: string
  /** Defaults to the current theme's primary text color. */
  tintColor?: string
  style?: StyleProp<ViewStyle>
}

/**
 * The chevron back/cancel control shared by every custom screen header.
 * Bakes in the platform-specific SF Symbol / Material icon names so call
 * sites don't repeat them.
 */
export function BackButton({
  onPress,
  accessibilityLabel = 'Back',
  tintColor,
  style,
}: BackButtonProps): React.ReactElement {
  const router = useRouter()
  const theme = useTheme()

  return (
    <Pressable
      onPress={onPress ?? (() => router.back())}
      style={({ pressed }) => [styles.iconBack, style, pressed && styles.pressed]}
      accessibilityLabel={accessibilityLabel}
    >
      <SymbolView
        name={BACK_ICON}
        size={20}
        weight="semibold"
        tintColor={tintColor ?? theme.text}
      />
    </Pressable>
  )
}

const styles = StyleSheet.create({
  iconBack: {
    width: Spacing.five,
    height: Spacing.five,
    alignItems: 'center',
    justifyContent: 'center',
  },
  pressed: {
    opacity: 0.6,
  },
})
