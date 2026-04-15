import { useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { Pressable, StyleSheet, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { TERMINAL_THEME_PRESETS, type TerminalThemePreset } from '@/constants/terminal-themes'
import { Spacing } from '@/constants/theme'
import { useAppPreferences } from '@/hooks/use-app-preferences'
import { useTheme } from '@/hooks/use-theme'
import { AppPreferencesStorage } from '@/lib/storage/app-preferences'
import type { AppThemeMode } from '@/lib/storage/app-preferences-types'

const APP_THEME_OPTIONS: ReadonlyArray<{ id: AppThemeMode; label: string }> = [
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
  { id: 'system', label: 'System' },
]

const TERMINAL_MODE_OPTIONS: ReadonlyArray<{ id: AppThemeMode; label: string }> = [
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
  { id: 'system', label: 'Follow app' },
]

export default function AppearanceScreen(): React.ReactElement {
  const router = useRouter()
  const theme = useTheme()
  const { appTheme, terminalThemeMode, terminalThemeId } = useAppPreferences()

  const setAppTheme = (mode: AppThemeMode): void => {
    void AppPreferencesStorage.set({ appTheme: mode }).catch(() => {
      /* best-effort; stays in memory either way */
    })
  }

  const setTerminalThemeMode = (mode: AppThemeMode): void => {
    void AppPreferencesStorage.set({ terminalThemeMode: mode }).catch(() => {
      /* best-effort */
    })
  }

  const setTerminalTheme = (id: TerminalThemePreset['id']): void => {
    void AppPreferencesStorage.set({ terminalThemeId: id }).catch(() => {
      /* best-effort */
    })
  }

  return (
    <ThemedView style={styles.container}>
      <SafeAreaView style={styles.safeArea} edges={['top']}>
        <View style={styles.header}>
          <Pressable
            onPress={() => router.back()}
            style={({ pressed }) => [styles.iconBack, pressed && styles.pressed]}
            accessibilityLabel="Back"
          >
            <SymbolView
              name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
              size={20}
              weight="semibold"
              tintColor={theme.text}
            />
          </Pressable>
          <View style={styles.titleWrap}>
            <ThemedText type="subtitle" numberOfLines={1}>
              Appearance
            </ThemedText>
          </View>
        </View>

        <View style={styles.section}>
          <ThemedText type="smallBold" themeColor="textSecondary" style={styles.sectionLabel}>
            APP THEME
          </ThemedText>
          <ThemedView type="backgroundElement" style={styles.card}>
            {APP_THEME_OPTIONS.map((opt, idx) => (
              <View key={opt.id}>
                {idx > 0 ? <View style={styles.divider} /> : null}
                <Pressable
                  onPress={() => setAppTheme(opt.id)}
                  style={({ pressed }) => [styles.row, pressed && styles.pressed]}
                  accessibilityRole="button"
                  accessibilityState={{ selected: appTheme === opt.id }}
                >
                  <ThemedText type="small">{opt.label}</ThemedText>
                  {appTheme === opt.id ? (
                    <SymbolView
                      name={{ ios: 'checkmark', android: 'check', web: 'check' }}
                      size={16}
                      weight="semibold"
                      tintColor={theme.text}
                    />
                  ) : null}
                </Pressable>
              </View>
            ))}
          </ThemedView>
        </View>

        <View style={styles.section}>
          <ThemedText type="smallBold" themeColor="textSecondary" style={styles.sectionLabel}>
            TERMINAL MODE
          </ThemedText>
          <ThemedView type="backgroundElement" style={styles.card}>
            {TERMINAL_MODE_OPTIONS.map((opt, idx) => (
              <View key={opt.id}>
                {idx > 0 ? <View style={styles.divider} /> : null}
                <Pressable
                  onPress={() => setTerminalThemeMode(opt.id)}
                  style={({ pressed }) => [styles.row, pressed && styles.pressed]}
                  accessibilityRole="button"
                  accessibilityState={{ selected: terminalThemeMode === opt.id }}
                >
                  <ThemedText type="small">{opt.label}</ThemedText>
                  {terminalThemeMode === opt.id ? (
                    <SymbolView
                      name={{ ios: 'checkmark', android: 'check', web: 'check' }}
                      size={16}
                      weight="semibold"
                      tintColor={theme.text}
                    />
                  ) : null}
                </Pressable>
              </View>
            ))}
          </ThemedView>
        </View>

        <View style={styles.section}>
          <ThemedText type="smallBold" themeColor="textSecondary" style={styles.sectionLabel}>
            TERMINAL THEME
          </ThemedText>
          <ThemedView type="backgroundElement" style={styles.card}>
            {TERMINAL_THEME_PRESETS.map((preset, idx) => (
              <View key={preset.id}>
                {idx > 0 ? <View style={styles.divider} /> : null}
                <Pressable
                  onPress={() => setTerminalTheme(preset.id)}
                  style={({ pressed }) => [styles.row, pressed && styles.pressed]}
                  accessibilityRole="button"
                  accessibilityState={{ selected: terminalThemeId === preset.id }}
                >
                  <ThemedText type="small">{preset.label}</ThemedText>
                  <View style={styles.presetRight}>
                    <View style={styles.swatches}>
                      <View style={[styles.swatch, { backgroundColor: preset.light.background }]} />
                      <View style={[styles.swatch, { backgroundColor: preset.dark.background }]} />
                    </View>
                    {terminalThemeId === preset.id ? (
                      <SymbolView
                        name={{ ios: 'checkmark', android: 'check', web: 'check' }}
                        size={16}
                        weight="semibold"
                        tintColor={theme.text}
                      />
                    ) : null}
                  </View>
                </Pressable>
              </View>
            ))}
          </ThemedView>
        </View>
      </SafeAreaView>
    </ThemedView>
  )
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  safeArea: {
    flex: 1,
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
    paddingHorizontal: Spacing.four,
    paddingTop: Spacing.three,
    paddingBottom: Spacing.three,
  },
  iconBack: {
    width: Spacing.five,
    height: Spacing.five,
    alignItems: 'center',
    justifyContent: 'center',
  },
  titleWrap: {
    flex: 1,
  },
  section: {
    paddingHorizontal: Spacing.four,
    gap: Spacing.two,
    marginTop: Spacing.three,
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
  divider: {
    height: StyleSheet.hairlineWidth,
    backgroundColor: 'rgba(127, 127, 127, 0.2)',
    marginLeft: Spacing.three,
  },
  presetRight: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
  },
  swatches: {
    flexDirection: 'row',
    gap: Spacing.one,
  },
  swatch: {
    width: 18,
    height: 18,
    borderRadius: 4,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: 'rgba(127, 127, 127, 0.4)',
  },
  pressed: {
    opacity: 0.6,
  },
})
