import { useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { useState } from 'react'
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, TextInput, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { Spacing } from '@/constants/theme'
import { useRemoteSession } from '@/hooks/use-remote-session'
import { useTheme } from '@/hooks/use-theme'

export default function AttachProjectScreen(): React.ReactElement {
  const router = useRouter()
  const theme = useTheme()
  const { api } = useRemoteSession()

  const [path, setPath] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const canSubmit = !!api && path.trim().length > 0 && !submitting

  const submit = async (): Promise<void> => {
    if (!canSubmit || !api) return
    setSubmitting(true)
    setError(null)
    try {
      await api.project.attach(path.trim())
      router.back()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setSubmitting(false)
    }
  }

  if (!api) {
    return (
      <ThemedView style={styles.centered}>
        <ThemedText type="subtitle">Not connected</ThemedText>
        <ThemedText type="small" themeColor="textSecondary" style={styles.hint}>
          Open the instance first to establish a session.
        </ThemedText>
        <Pressable
          onPress={() => router.back()}
          style={({ pressed }) => [styles.linkButton, pressed && styles.pressed]}
        >
          <ThemedText type="linkPrimary">Go back</ThemedText>
        </Pressable>
      </ThemedView>
    )
  }

  return (
    <ThemedView style={styles.container}>
      <SafeAreaView style={styles.safeArea} edges={['top']}>
        <View style={styles.header}>
          <Pressable
            onPress={() => router.back()}
            style={({ pressed }) => [styles.iconBack, pressed && styles.pressed]}
            accessibilityLabel="Cancel"
          >
            <SymbolView
              name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
              size={20}
              weight="semibold"
              tintColor={theme.text}
            />
          </Pressable>
          <View style={styles.headerTitle}>
            <ThemedText type="subtitle">Attach project</ThemedText>
          </View>
        </View>

        <ScrollView
          contentContainerStyle={styles.scrollContent}
          keyboardShouldPersistTaps="handled"
        >
          <ThemedText type="smallBold" themeColor="textSecondary" style={styles.fieldLabel}>
            PROJECT PATH
          </ThemedText>
          <ThemedView
            type="backgroundElement"
            style={[styles.inputWrap, submitting && styles.disabled]}
          >
            <TextInput
              value={path}
              onChangeText={setPath}
              placeholder="~/code/my-project"
              placeholderTextColor={theme.textSecondary}
              autoCapitalize="none"
              autoCorrect={false}
              autoFocus
              editable={!submitting}
              style={[styles.input, styles.inputMono, { color: theme.text }]}
            />
          </ThemedView>
          <ThemedText type="small" themeColor="textSecondary" style={styles.hintSmall}>
            Enter an absolute path or use ~ for your home directory.
          </ThemedText>

          {error && (
            <ThemedView type="backgroundElement" style={styles.errorBox}>
              <ThemedText type="small" style={styles.errorText}>
                {error}
              </ThemedText>
            </ThemedView>
          )}

          <View style={styles.actions}>
            <Pressable
              onPress={() => router.back()}
              style={({ pressed }) => [styles.actionBtn, pressed && styles.pressed]}
            >
              <ThemedView type="backgroundElement" style={styles.actionInner}>
                <ThemedText type="smallBold">Cancel</ThemedText>
              </ThemedView>
            </Pressable>
            <Pressable
              onPress={submit}
              disabled={!canSubmit}
              style={({ pressed }) => [styles.actionBtn, pressed && styles.pressed]}
            >
              <ThemedView
                type="backgroundSelected"
                style={[styles.actionInner, !canSubmit && styles.disabled]}
              >
                {submitting ? (
                  <ActivityIndicator />
                ) : (
                  <ThemedText type="smallBold">Attach</ThemedText>
                )}
              </ThemedView>
            </Pressable>
          </View>
        </ScrollView>
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
  centered: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: Spacing.two,
    padding: Spacing.four,
  },
  hint: {
    textAlign: 'center',
  },
  linkButton: {
    marginTop: Spacing.three,
    padding: Spacing.two,
  },
  header: {
    flexDirection: 'row',
    paddingHorizontal: Spacing.four,
    paddingTop: Spacing.three,
    gap: Spacing.three,
  },
  iconBack: {
    width: Spacing.five,
    height: Spacing.five,
    alignItems: 'center',
    justifyContent: 'center',
    marginTop: Spacing.one,
  },
  headerTitle: {
    flex: 1,
    gap: Spacing.half,
  },
  scrollContent: {
    padding: Spacing.four,
    gap: Spacing.two,
    paddingBottom: Spacing.six,
  },
  fieldLabel: {
    fontSize: 11,
    letterSpacing: 0.5,
    marginTop: Spacing.three,
  },
  inputWrap: {
    borderRadius: Spacing.three,
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.two,
  },
  input: {
    fontSize: 14,
    paddingVertical: Spacing.one,
  },
  inputMono: {
    fontFamily: 'Menlo',
    fontSize: 12,
  },
  hintSmall: {
    paddingHorizontal: Spacing.one,
  },
  errorBox: {
    borderRadius: Spacing.three,
    padding: Spacing.three,
    borderWidth: 1,
    borderColor: '#ff453a',
  },
  errorText: {
    color: '#ff453a',
  },
  actions: {
    flexDirection: 'row',
    gap: Spacing.three,
    marginTop: Spacing.four,
  },
  actionBtn: {
    flex: 1,
  },
  actionInner: {
    paddingVertical: Spacing.three,
    borderRadius: Spacing.three,
    alignItems: 'center',
  },
  disabled: {
    opacity: 0.5,
  },
  pressed: {
    opacity: 0.6,
  },
})
