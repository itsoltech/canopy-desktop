import { ActivityIndicator, Pressable } from 'react-native'

import { ThemedText } from './themed-text'

type HeaderButtonProps = {
  label: string
  onPress: () => void
  disabled?: boolean
  /** Render a spinner instead of the label (e.g. while submitting). */
  loading?: boolean
  /** Emphasize as the primary action. */
  bold?: boolean
}

/**
 * Text action for a native Stack header (`headerLeft` / `headerRight`) — e.g.
 * Cancel / Attach / Create on modal screens. Monochrome to match the app theme.
 */
export function HeaderButton({
  label,
  onPress,
  disabled = false,
  loading = false,
  bold = false,
}: HeaderButtonProps): React.ReactElement {
  if (loading) return <ActivityIndicator />
  return (
    <Pressable onPress={onPress} disabled={disabled} hitSlop={8}>
      {({ pressed }) => (
        <ThemedText
          type="default"
          themeColor={disabled ? 'textSecondary' : 'text'}
          style={[bold ? { fontWeight: '700' } : null, { opacity: pressed ? 0.4 : 1 }]}
        >
          {label}
        </ThemedText>
      )}
    </Pressable>
  )
}
