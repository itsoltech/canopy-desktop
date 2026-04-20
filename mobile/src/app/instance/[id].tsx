import { useLocalSearchParams, useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { useEffect, useRef, useState } from 'react'
import { Alert, Pressable, ScrollView, StyleSheet, TextInput, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { ProjectSection } from '@/components/worktrees/project-section'
import { Spacing } from '@/constants/theme'
import { useRemoteSession } from '@/hooks/use-remote-session'
import { useProjects } from '@/hooks/use-remote-state'
import { useSavedInstance } from '@/hooks/use-saved-instances'
import { useTheme } from '@/hooks/use-theme'
import type { WorktreeSnapshot } from '@/lib/remote/protocol/state-snapshot'
import { SavedInstancesStorage } from '@/lib/storage/saved-instances'

export default function InstanceDetailScreen(): React.ReactElement {
  const { id } = useLocalSearchParams<{ id: string }>()
  const router = useRouter()
  const theme = useTheme()
  const { instance, loading } = useSavedInstance(id)
  const { state: sessionState, connect, disconnect } = useRemoteSession()
  const projects = useProjects()
  const [editing, setEditing] = useState(false)
  const [draftNickname, setDraftNickname] = useState('')

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
      <ThemedView style={styles.centered}>
        <ThemedText type="small" themeColor="textSecondary">
          Loading…
        </ThemedText>
      </ThemedView>
    )
  }

  if (!instance) {
    return (
      <ThemedView style={styles.centered}>
        <ThemedText type="subtitle">Not found</ThemedText>
        <ThemedText type="small" themeColor="textSecondary" style={styles.notFoundHint}>
          This instance no longer exists.
        </ThemedText>
        <Pressable
          onPress={() => router.back()}
          style={({ pressed }) => [styles.backButton, pressed && styles.pressed]}
        >
          <ThemedText type="linkPrimary">Go back</ThemedText>
        </Pressable>
      </ThemedView>
    )
  }

  const startEdit = (): void => {
    setDraftNickname(instance.nickname)
    setEditing(true)
  }

  const commitEdit = async (): Promise<void> => {
    const trimmed = draftNickname.trim()
    setEditing(false)
    if (!trimmed || trimmed === instance.nickname) return
    try {
      await SavedInstancesStorage.update(instance.id, { nickname: trimmed })
    } catch (e) {
      Alert.alert('Could not rename', e instanceof Error ? e.message : String(e))
    }
  }

  const onWorktreePress = (_: unknown, worktree: WorktreeSnapshot): void => {
    router.push({
      pathname: '/terminal',
      params: {
        instanceId: instance.id,
        worktreePath: encodeURIComponent(worktree.path),
      },
    })
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

  const banner = sessionBannerText(sessionState)

  return (
    <ThemedView style={styles.container}>
      <SafeAreaView style={styles.safeArea} edges={['top']}>
        <ScrollView contentContainerStyle={styles.scrollContent}>
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

            <View style={styles.headerTitle}>
              {editing ? (
                <TextInput
                  value={draftNickname}
                  onChangeText={setDraftNickname}
                  onBlur={commitEdit}
                  onSubmitEditing={commitEdit}
                  autoFocus
                  selectTextOnFocus
                  style={[styles.nicknameInput, { color: theme.text }]}
                />
              ) : (
                <Pressable onPress={startEdit} style={styles.nicknamePress}>
                  <ThemedText type="subtitle" numberOfLines={1} style={styles.nickname}>
                    {instance.nickname}
                  </ThemedText>
                  <SymbolView
                    name={{ ios: 'pencil', android: 'edit', web: 'edit' }}
                    size={14}
                    weight="regular"
                    tintColor={theme.textSecondary}
                  />
                </Pressable>
              )}
              <ThemedText type="small" themeColor="textSecondary" numberOfLines={1}>
                {instance.hostname}
              </ThemedText>
              <ThemedText type="code" themeColor="textSecondary" numberOfLines={1}>
                {instance.lanIp}:{instance.port}
              </ThemedText>
            </View>
          </View>

          {banner ? (
            <View style={styles.bannerWrap}>
              <ThemedView type="backgroundElement" style={styles.banner}>
                <ThemedText type="small" themeColor="textSecondary">
                  {banner}
                </ThemedText>
              </ThemedView>
            </View>
          ) : null}

          <View style={styles.sectionLabelRow}>
            <ThemedText type="smallBold" themeColor="textSecondary" style={styles.sectionLabel}>
              PROJECTS & WORKTREES
            </ThemedText>
            {sessionState.kind === 'ready' && (
              <Pressable
                onPress={() => router.push('/project/new' as never)}
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

          <View style={styles.projects}>
            {projects.length === 0 ? (
              <ThemedView type="backgroundElement" style={styles.empty}>
                <ThemedText type="small" themeColor="textSecondary">
                  {sessionState.kind === 'ready'
                    ? 'No projects open on the host'
                    : 'Waiting for host data…'}
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
          </View>

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
      </SafeAreaView>
    </ThemedView>
  )
}

function sessionBannerText(state: ReturnType<typeof useRemoteSession>['state']): string | null {
  switch (state.kind) {
    case 'idle':
      return null
    case 'connecting':
      switch (state.phase) {
        case 'signaling':
          return 'Connecting to host…'
        case 'pairing':
          return 'Validating pairing token…'
        case 'awaiting-accept':
          return 'Waiting for approval on desktop…'
        case 'rtc':
          return 'Establishing WebRTC connection…'
      }
      return 'Connecting…'
    case 'reconnecting':
      return 'Reconnecting…'
    case 'ready':
      return null
    case 'error':
      return `Connection error: ${state.message}`
    case 'disconnected':
      return 'Disconnected'
  }
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  safeArea: {
    flex: 1,
  },
  scrollContent: {
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
  backButton: {
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
  nicknamePress: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
  },
  nickname: {
    fontSize: 24,
    lineHeight: 30,
    flexShrink: 1,
  },
  nicknameInput: {
    fontSize: 24,
    lineHeight: 30,
    fontWeight: '600',
    paddingVertical: 0,
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
