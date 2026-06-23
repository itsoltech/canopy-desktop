import { StyleSheet, View, type StyleProp, type ViewStyle } from 'react-native'

import { Spacing } from '@/constants/theme'

import { BackButton } from './back-button'

type ScreenHeaderProps = {
  /** Title column content — pass themed `<ThemedText>` nodes so each screen keeps its own typography. */
  children?: React.ReactNode
  /** Optional trailing action rendered after the title column. */
  right?: React.ReactNode
  /** Defaults to `router.back()`. */
  onBack?: () => void
  backAccessibilityLabel?: string
  /**
   * `top` aligns the back button to the first title line (multi-line titles);
   * `center` vertically centers everything (compact single-line headers).
   */
  align?: 'top' | 'center'
  style?: StyleProp<ViewStyle>
}

/**
 * Custom screen header: back button + title column (+ optional trailing slot).
 * Owns the shared header/back/title layout that was previously copy-pasted
 * across the detail and modal screens.
 */
export function ScreenHeader({
  children,
  right,
  onBack,
  backAccessibilityLabel = 'Back',
  align = 'top',
  style,
}: ScreenHeaderProps): React.ReactElement {
  return (
    <View style={[styles.header, align === 'center' && styles.headerCentered, style]}>
      <BackButton
        onPress={onBack}
        accessibilityLabel={backAccessibilityLabel}
        style={align === 'top' ? styles.backTopAlign : undefined}
      />
      <View style={styles.title}>{children}</View>
      {right}
    </View>
  )
}

const styles = StyleSheet.create({
  header: {
    flexDirection: 'row',
    paddingHorizontal: Spacing.four,
    paddingTop: Spacing.three,
    gap: Spacing.three,
  },
  headerCentered: {
    alignItems: 'center',
  },
  backTopAlign: {
    marginTop: Spacing.one,
  },
  title: {
    flex: 1,
    gap: Spacing.half,
  },
})
