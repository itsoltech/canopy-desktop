import { Stack, useLocalSearchParams, useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { useEffect, useRef } from 'react'
import { Alert, Pressable, ScrollView, StyleSheet, View } from 'react-native'
import Animated, { FadeIn, FadeOut, useReducedMotion } from 'react-native-reanimated'

import { ProjectsSkeleton } from '@/components/instances/projects-skeleton'
import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { ProjectSection } from '@/components/worktrees/project-section'
import { Spacing } from '@/constants/theme'
import { useRemoteSession } from '@/hooks/use-remote-session'
import { useIsHydrated, useProjects } from '@/hooks/use-remote-state'
import { useSavedInstance } from '@/hooks/use-saved-instances'
import { useTheme } from '@/hooks/use-theme'
import { AppRoutes } from '@/lib/navigation/routes'
import type { WorktreeSnapshot } from '@/lib/remote/protocol/state-snapshot'
import { sessionBannerText, shouldShowBanner } from '@/lib/remote/session-status'
import { SavedInstancesStorage } from '@/lib/storage/saved-instances'

export default function InstanceDetailScreen(): React.ReactElement {
  const { id } = useLocalSearchParams<{ id: string }>()
  const router = useRouter()
  const theme = useTheme()
  const { instance, loading } = useSavedInstance(id)
  const { state: sessionState, connect, disconnect } = useRemoteSession()
  const projects = useProjects()
  const hydrated = useIsHydrated()
  const reduceMotion = useReducedMotion()

  // Capture instance in a ref so we can call connect() without having
  // `instance` in the dep list — otherwise every SavedInstancesStorage
  // mutation (e.g. a rename) rebuilds the effect and tears down the live
  // RTC session. The effect should only re-run when the instance ID
  // actually changes.
  const instanceRef = useRef(instance)
  instanceRef.current = instance
  const instanceId = instance?.id
  useEffect(() => {
    const inst = instanceRef.current
    if (!inst) return
    void connect(inst)
    return () => {
      void disconnect()
    }
  }, [instanceId, connect, disconnect])

  if (loading) {
    return (
      <>
        <Stack.Screen options={{ title: '' }} />
        <ScrollView
          style={styles.scroll}
          contentInsetAdjustmentBehavior="automatic"
          contentContainerStyle={styles.scrollContent}
        >
          <ProjectsSkeleton />
        </ScrollView>
      </>
    )
  }

  if (!instance) {
    return (
      <ThemedView style={styles.centered}>
        <Stack.Screen options={{ title: 'Not found' }} />
        <ThemedText type="subtitle">Not found</ThemedText>
        <ThemedText type="small" themeColor="textSecondary" style={styles.notFoundHint}>
          This instance no longer exists.
        </ThemedText>
      </ThemedView>
    )
  }

  // Native iOS rename prompt. `Alert.prompt` is iOS-only; on Android it is a
  // no-op (acceptable — this is the iOS companion app).
  const promptRename = (): void => {
    Alert.prompt(
      'Rename instance',
      undefined,
      (text) => {
        const trimmed = text?.trim()
        if (!trimmed || trimmed === instance.nickname) return
        void SavedInstancesStorage.update(instance.id, { nickname: trimmed }).catch(
          (e: unknown) => {
            Alert.alert('Could not rename', e instanceof Error ? e.message : String(e))
          },
        )
      },
      'plain-text',
      instance.nickname,
    )
  }

  const onWorktreePress = (_: unknown, worktree: WorktreeSnapshot): void => {
    router.push(AppRoutes.terminal(instance.id, worktree.path))
  }

  const confirmRemove = (): void => {
    Alert.alert('Remove instance', `Remove "${instance.nickname}" from saved instances?`, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Remove',
        style: 'destructive',
        onPress: async () => {
          await SavedInstancesStorage.remove(instance.id)
          router.back()
        },
      },
    ])
  }

  const banner = shouldShowBanner(sessionState) ? sessionBannerText(sessionState) : null

  return (
    <>
      <Stack.Screen options={{ headerLargeTitle: true, title: instance.nickname }} />
      <ScrollView
        style={styles.scroll}
        contentInsetAdjustmentBehavior="automatic"
        contentContainerStyle={styles.scrollContent}
      >
        <View style={styles.metaRow}>
          <View style={styles.metaText}>
            <ThemedText type="small" themeColor="textSecondary" numberOfLines={1}>
              {instance.hostname}
            </ThemedText>
            <ThemedText type="code" themeColor="textSecondary" numberOfLines={1}>
              {instance.lanIp}:{instance.port}
            </ThemedText>
          </View>
          <Pressable
            onPress={promptRename}
            style={({ pressed }) => [styles.renameBtn, pressed && styles.pressed]}
            accessibilityLabel="Rename instance"
          >
            <SymbolView
              name={{ ios: 'pencil', android: 'edit', web: 'edit' }}
              size={12}
              weight="semibold"
              tintColor={theme.text}
            />
            <ThemedText type="smallBold">Rename</ThemedText>
          </Pressable>
        </View>

        {banner ? (
          <Animated.View
            entering={reduceMotion ? undefined : FadeIn}
            exiting={reduceMotion ? undefined : FadeOut}
            style={styles.bannerWrap}
          >
            <ThemedView type="backgroundElement" style={styles.banner}>
              <ThemedText type="small" themeColor="textSecondary">
                {banner}
              </ThemedText>
            </ThemedView>
          </Animated.View>
        ) : null}

        <View style={styles.sectionLabelRow}>
          <ThemedText type="smallBold" themeColor="textSecondary" style={styles.sectionLabel}>
            PROJECTS & WORKTREES
          </ThemedText>
          {hydrated && (
            <Pressable
              onPress={() => router.push(AppRoutes.projectNew())}
              style={({ pressed }) => [styles.addProjectBtn, pressed && styles.pressed]}
              accessibilityLabel="Attach project"
            >
              <SymbolView
                name={{ ios: 'plus', android: 'add', web: 'add' }}
                size={12}
                weight="semibold"
                tintColor={theme.text}
              />
              <ThemedText type="smallBold">project</ThemedText>
            </Pressable>
          )}
        </View>

        {hydrated ? (
          <Animated.View style={styles.projects} entering={reduceMotion ? undefined : FadeIn}>
            {projects.length === 0 ? (
              <ThemedView type="backgroundElement" style={styles.empty}>
                <ThemedText type="small" themeColor="textSecondary">
                  No projects open on the host
                </ThemedText>
              </ThemedView>
            ) : (
              projects.map((project) => (
                <ProjectSection
                  key={project.id}
                  project={project}
                  onWorktreePress={onWorktreePress}
                />
              ))
            )}
          </Animated.View>
        ) : sessionState.kind === 'error' || sessionState.kind === 'disconnected' ? (
          // First-connect failure before any snapshot hydrated (bad token, host
          // offline, RTC failure): show the reason in-content with a Retry,
          // instead of leaving the region blank under the thin banner.
          <View style={styles.projects}>
            <ThemedView type="backgroundElement" style={styles.errorBox}>
              <ThemedText type="small" themeColor="textSecondary" style={styles.errorText}>
                {sessionState.kind === 'error'
                  ? sessionState.message
                  : 'Disconnected from the host.'}
              </ThemedText>
              <Pressable
                onPress={() => void connect(instance)}
                style={({ pressed }) => [styles.retryBtn, pressed && styles.pressed]}
                accessibilityRole="button"
                accessibilityLabel="Retry connection"
              >
                <SymbolView
                  name={{ ios: 'arrow.clockwise', android: 'refresh', web: 'refresh' }}
                  size={14}
                  weight="semibold"
                  tintColor={theme.text}
                />
                <ThemedText type="smallBold">Retry</ThemedText>
              </Pressable>
            </ThemedView>
          </View>
        ) : (
          <ProjectsSkeleton />
        )}

        <Pressable
          onPress={confirmRemove}
          style={({ pressed }) => [styles.removeButton, pressed && styles.pressed]}
        >
          <ThemedView type="backgroundElement" style={styles.removeInner}>
            <SymbolView
              name={{ ios: 'trash', android: 'delete', web: 'delete' }}
              size={16}
              weight="semibold"
              tintColor="#e5484d"
            />
            <ThemedText type="smallBold" style={styles.removeText}>
              Remove instance
            </ThemedText>
          </ThemedView>
        </Pressable>
      </ScrollView>
    </>
  )
}

const styles = StyleSheet.create({
  scroll: {
    flex: 1,
  },
  scrollContent: {
    paddingTop: Spacing.two,
    paddingBottom: Spacing.six,
    gap: Spacing.four,
  },
  centered: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: Spacing.two,
    padding: Spacing.four,
  },
  notFoundHint: {
    textAlign: 'center',
  },
  metaRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: Spacing.three,
    paddingHorizontal: Spacing.four,
  },
  metaText: {
    flex: 1,
    gap: Spacing.half,
  },
  renameBtn: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.one,
    paddingHorizontal: Spacing.two,
    paddingVertical: Spacing.one,
    borderRadius: Spacing.two,
    backgroundColor: 'rgba(127, 127, 127, 0.15)',
  },
  bannerWrap: {
    paddingHorizontal: Spacing.four,
  },
  banner: {
    borderRadius: Spacing.three,
    padding: Spacing.three,
  },
  sectionLabelRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: Spacing.four,
  },
  sectionLabel: {
    fontSize: 12,
    letterSpacing: 0.5,
  },
  addProjectBtn: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.one,
    paddingHorizontal: Spacing.two,
    paddingVertical: Spacing.one,
    borderRadius: Spacing.two,
    backgroundColor: 'rgba(127, 127, 127, 0.15)',
  },
  projects: {
    paddingHorizontal: Spacing.four,
    gap: Spacing.four,
  },
  empty: {
    borderRadius: Spacing.three,
    padding: Spacing.three,
    alignItems: 'center',
  },
  errorBox: {
    borderRadius: Spacing.three,
    padding: Spacing.three,
    alignItems: 'center',
    gap: Spacing.two,
  },
  errorText: {
    textAlign: 'center',
  },
  retryBtn: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.one,
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.one,
    borderRadius: Spacing.two,
    backgroundColor: 'rgba(127, 127, 127, 0.15)',
  },
  removeButton: {
    marginTop: Spacing.three,
    paddingHorizontal: Spacing.four,
  },
  removeInner: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: Spacing.two,
    paddingVertical: Spacing.three,
    borderRadius: Spacing.three,
  },
  removeText: {
    color: '#e5484d',
  },
  pressed: {
    opacity: 0.6,
  },
})
