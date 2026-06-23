import Constants from 'expo-constants'
import { useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { Pressable, ScrollView, StyleSheet, View } from 'react-native'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { AppConfig } from '@/config/app-config'
import { BottomTabInset, Spacing } from '@/constants/theme'
import { useAppPreferences } from '@/hooks/use-app-preferences'
import { useTheme } from '@/hooks/use-theme'
import { AppRoutes } from '@/lib/navigation/routes'
import type { AppThemeMode } from '@/lib/storage/app-preferences-types'

const APP_THEME_LABELS: Record<AppThemeMode, string> = {
  light: 'Light',
  dark: 'Dark',
  system: 'System',
}

export default function SettingsScreen(): React.ReactElement {
  const version = Constants.expoConfig?.version ?? '—'
  const router = useRouter()
  const theme = useTheme()
  const { appTheme } = useAppPreferences()

  // The ScrollView must be the screen's direct first child so the native
  // large title tracks/collapses against it. Wrapping it in a View breaks
  // large-title pinning (the title scrolls over content instead). Background
  // comes from the stack's contentStyle.
  return (
    <ScrollView
      style={styles.scroll}
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={styles.scrollContent}
    >
      <View style={styles.section}>
        <ThemedText type="smallBold" themeColor="textSecondary" style={styles.sectionLabel}>
          APPEARANCE
        </ThemedText>
        <ThemedView type="backgroundElement" style={styles.card}>
          <Pressable
            onPress={() => router.push(AppRoutes.settingsAppearance())}
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
    </ScrollView>
  )
}

function SettingsRow({ label, value }: { label: string; value: string }): React.ReactElement {
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
  scroll: {
    flex: 1,
  },
  scrollContent: {
    paddingTop: Spacing.two,
    paddingBottom: BottomTabInset + Spacing.four,
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
