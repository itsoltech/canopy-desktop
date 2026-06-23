import { useRouter } from 'expo-router'
import { Pressable, StyleSheet } from 'react-native'

import { Spacing } from '@/constants/theme'

import { ThemedText } from './themed-text'
import { ThemedView } from './themed-view'

type ConnectionFallbackProps = {
  /** Optional explanation rendered under the "Not connected" title. */
  hint?: string
}

/**
 * Consistent "Not connected" screen shown by connection-dependent modals
 * (attach project, new worktree) when no live session is available. Replaces
 * the per-screen copies; render it from the screen's own `if (!api)` guard so
 * `api` stays narrowed to non-null afterwards.
 */
export function ConnectionFallback({ hint }: ConnectionFallbackProps): React.ReactElement {
  const router = useRouter()

  return (
    <ThemedView style={styles.centered}>
      <ThemedText type="subtitle">Not connected</ThemedText>
      {hint ? (
        <ThemedText type="small" themeColor="textSecondary" style={styles.hint}>
          {hint}
        </ThemedText>
      ) : null}
      <Pressable
        onPress={() => router.back()}
        style={({ pressed }) => [styles.linkButton, pressed && styles.pressed]}
      >
        <ThemedText type="linkPrimary">Go back</ThemedText>
      </Pressable>
    </ThemedView>
  )
}

const styles = StyleSheet.create({
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
  pressed: {
    opacity: 0.6,
  },
})
