import Constants from 'expo-constants'
import { useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { Pressable, StyleSheet, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { AppConfig } from '@/config/app-config'
import { BottomTabInset, Spacing } from '@/constants/theme'
import { useAppPreferences } from '@/hooks/use-app-preferences'
import { useTheme } from '@/hooks/use-theme'
import type { AppThemeMode } from '@/lib/storage/app-preferences-types'

const APP_THEME_LABELS: Record<AppThemeMode, string> = {
  light: 'Light',
  dark: 'Dark',
  system: 'System',
}

export default function SettingsScreen() {
  const version = Constants.expoConfig?.version ?? '—'
  const router = useRouter()
  const theme = useTheme()
  const { appTheme } = useAppPreferences()

  return (
    <ThemedView style={styles.container}>
      <SafeAreaView style={styles.safeArea} edges={['top']}>
        <View style={styles.header}>
          <ThemedText type="subtitle">Settings</ThemedText>
        </View>

        <View style={styles.section}>
          <ThemedText type="smallBold" themeColor="textSecondary" style={styles.sectionLabel}>
            APPEARANCE
          </ThemedText>
          <ThemedView type="backgroundElement" style={styles.card}>
            <Pressable
              onPress={() => router.push('/settings/appearance')}
              style={({ pressed }) => [styles.row, pressed && styles.pressed]}
              accessibilityRole="button"
            >
              <ThemedText type="small">Appearance</ThemedText>
              <View style={styles.rowRight}>
                <ThemedText type="small" themeColor="textSecondary">
                  {APP_THEME_LABELS[appTheme]}
                </ThemedText>
                <SymbolView
                  name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
                  size={14}
                  weight="semibold"
                  tintColor={theme.textSecondary}
                />
              </View>
            </Pressable>
          </ThemedView>
        </View>

        <View style={styles.section}>
          <ThemedText type="smallBold" themeColor="textSecondary" style={styles.sectionLabel}>
            ABOUT
          </ThemedText>
          <ThemedView type="backgroundElement" style={styles.card}>
            <SettingsRow label="Version" value={version} />
            <View style={styles.divider} />
            <SettingsRow label="Channel" value={AppConfig.CHANNEL} />
          </ThemedView>
        </View>
      </SafeAreaView>
    </ThemedView>
  )
}

function SettingsRow({ label, value }: { label: string; value: string }) {
  return (
    <View style={styles.row}>
      <ThemedText type="small">{label}</ThemedText>
      <ThemedText type="code" themeColor="textSecondary">
        {value}
      </ThemedText>
    </View>
  )
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  safeArea: {
    flex: 1,
    paddingBottom: BottomTabInset,
  },
  header: {
    paddingHorizontal: Spacing.four,
    paddingVertical: Spacing.three,
  },
  section: {
    paddingHorizontal: Spacing.four,
    gap: Spacing.two,
    marginBottom: Spacing.three,
  },
  sectionLabel: {
    fontSize: 12,
    letterSpacing: 0.5,
    marginLeft: Spacing.three,
  },
  card: {
    borderRadius: Spacing.three,
    overflow: 'hidden',
  },
  row: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: Spacing.three,
    paddingHorizontal: Spacing.three,
  },
  rowRight: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
  },
  divider: {
    height: StyleSheet.hairlineWidth,
    backgroundColor: 'rgba(127, 127, 127, 0.2)',
    marginLeft: Spacing.three,
  },
  pressed: {
    opacity: 0.6,
  },
})
