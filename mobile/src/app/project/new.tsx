import { Stack, useRouter } from 'expo-router'
import { useState } from 'react'
import { ScrollView, StyleSheet, TextInput } from 'react-native'

import { ConnectionFallback } from '@/components/connection-fallback'
import { HeaderButton } from '@/components/header-button'
import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { DangerColor, Spacing } from '@/constants/theme'
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
    return <ConnectionFallback hint="Open the instance first to establish a session." />
  }

  return (
    <ThemedView style={styles.container}>
      <Stack.Screen
        options={{
          title: 'Attach project',
          headerLeft: () => <HeaderButton label="Cancel" onPress={() => router.back()} />,
          headerRight: () => (
            <HeaderButton
              label="Attach"
              onPress={submit}
              disabled={!canSubmit}
              loading={submitting}
              bold
            />
          ),
        }}
      />
      <ScrollView
        contentInsetAdjustmentBehavior="automatic"
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
      </ScrollView>
    </ThemedView>
  )
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
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
    borderColor: DangerColor,
  },
  errorText: {
    color: DangerColor,
  },
  disabled: {
    opacity: 0.5,
  },
})
