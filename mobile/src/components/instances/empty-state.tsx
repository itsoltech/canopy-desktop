import { SymbolView } from 'expo-symbols'
import { Pressable, StyleSheet, View } from 'react-native'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'

type EmptyStateProps = {
  onScanPress: () => void
  onAddMockPress?: () => void
}

export function InstancesEmptyState({
  onScanPress,
  onAddMockPress,
}: EmptyStateProps): React.ReactElement {
  const theme = useTheme()

  return (
    <View style={styles.container}>
      <ThemedView type="backgroundElement" style={styles.iconWrap}>
        <SymbolView
          name={{ ios: 'qrcode', android: 'qr_code', web: 'qr_code' }}
          size={48}
          weight="regular"
          tintColor={theme.textSecondary}
        />
      </ThemedView>
      <ThemedText type="subtitle" style={styles.title}>
        No instances yet
      </ThemedText>
      <ThemedText type="small" themeColor="textSecondary" style={styles.hint}>
        Scan a QR code from your Canopy desktop to pair with this device.
      </ThemedText>
      <Pressable
        onPress={onScanPress}
        style={({ pressed }) => [styles.primaryButton, pressed && styles.pressed]}
      >
        <ThemedView type="backgroundSelected" style={styles.primaryButtonInner}>
          <SymbolView
            name={{ ios: 'qrcode.viewfinder', android: 'qr_code_scanner', web: 'qr_code_scanner' }}
            size={18}
            weight="semibold"
            tintColor={theme.text}
          />
          <ThemedText type="smallBold">Scan QR</ThemedText>
        </ThemedView>
      </Pressable>
      {onAddMockPress && (
        <Pressable
          onPress={onAddMockPress}
          style={({ pressed }) => [styles.secondaryButton, pressed && styles.pressed]}
        >
          <ThemedText type="link" themeColor="textSecondary">
            Add mock instance (dev)
          </ThemedText>
        </Pressable>
      )}
    </View>
  )
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: Spacing.four,
    gap: Spacing.three,
  },
  iconWrap: {
    width: 96,
    height: 96,
    borderRadius: 48,
    alignItems: 'center',
    justifyContent: 'center',
  },
  title: {
    textAlign: 'center',
    fontSize: 24,
    lineHeight: 30,
  },
  hint: {
    textAlign: 'center',
    maxWidth: 280,
  },
  primaryButton: {
    marginTop: Spacing.two,
  },
  primaryButtonInner: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.four,
    borderRadius: Spacing.five,
  },
  secondaryButton: {
    paddingVertical: Spacing.one,
    paddingHorizontal: Spacing.two,
  },
  pressed: {
    opacity: 0.6,
  },
})
