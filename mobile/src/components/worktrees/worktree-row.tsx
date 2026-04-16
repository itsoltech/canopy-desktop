import { SymbolView } from 'expo-symbols'
import { useState } from 'react'
import { ActivityIndicator, Alert, Pressable, StyleSheet, View } from 'react-native'

import { ThemedText } from '@/components/themed-text'
import { Spacing, StatusColors } from '@/constants/theme'
import { useRemoteSession } from '@/hooks/use-remote-session'
import { useTheme } from '@/hooks/use-theme'
import type { WorktreeAgentStatus, WorktreeSnapshot } from '@/lib/mock/snapshot-types'

type WorktreeRowProps = {
  worktree: WorktreeSnapshot
  repoRoot: string | null
  onPress: () => void
}

const STATUS_COLORS: Record<Exclude<WorktreeAgentStatus, 'none'>, string> = StatusColors

export function WorktreeRow({ worktree, repoRoot, onPress }: WorktreeRowProps): React.ReactElement {
  const theme = useTheme()
  const { api } = useRemoteSession()
  const [removing, setRemoving] = useState(false)
  const canRemove = !worktree.isMain && repoRoot !== null && api !== null && !removing

  const confirmRemove = (): void => {
    if (!canRemove || !api || !repoRoot) return
    Alert.alert('Remove worktree', `Remove "${worktree.branch}"?`, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Remove',
        style: 'destructive',
        onPress: async () => {
          setRemoving(true)
          try {
            await api.worktree.remove({ repoRoot, path: worktree.path, force: false })
          } catch (e) {
            setRemoving(false)
            Alert.alert('Could not remove worktree', e instanceof Error ? e.message : String(e))
          }
        },
      },
    ])
  }

  const agentStatus = worktree.agentStatus ?? 'none'

  return (
    <Pressable
      onPress={removing ? undefined : onPress}
      style={({ pressed }) => [
        styles.row,
        pressed && !removing && styles.pressed,
        removing && styles.dimmed,
      ]}
    >
      {agentStatus !== 'none' ? (
        <View style={[styles.statusDot, { backgroundColor: STATUS_COLORS[agentStatus] }]} />
      ) : (
        <SymbolView
          name={
            worktree.isMain
              ? { ios: 'circle.fill', android: 'circle', web: 'circle' }
              : { ios: 'circle', android: 'radio_button_unchecked', web: 'radio_button_unchecked' }
          }
          size={12}
          weight="semibold"
          tintColor={worktree.isMain ? theme.text : theme.textSecondary}
        />
      )}
      <View style={styles.label}>
        <ThemedText type="small" numberOfLines={1}>
          {worktree.branch}
        </ThemedText>
        {worktree.isMain && (
          <ThemedText type="small" themeColor="textSecondary">
            main
          </ThemedText>
        )}
      </View>
      {removing ? (
        <ActivityIndicator size="small" style={styles.removeBtn} />
      ) : canRemove ? (
        <Pressable
          onPress={confirmRemove}
          hitSlop={8}
          style={({ pressed }) => [styles.removeBtn, pressed && styles.pressed]}
          accessibilityLabel={`Remove worktree ${worktree.branch}`}
        >
          <SymbolView
            name={{ ios: 'trash', android: 'delete', web: 'delete' }}
            size={14}
            weight="regular"
            tintColor="#ff453a"
          />
        </Pressable>
      ) : null}
      <SymbolView
        name={{ ios: 'chevron.right', android: 'chevron_right', web: 'chevron_right' }}
        size={12}
        weight="medium"
        tintColor={theme.textSecondary}
      />
    </Pressable>
  )
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.three,
  },
  pressed: {
    opacity: 0.5,
  },
  label: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
  },
  statusDot: {
    width: 10,
    height: 10,
    borderRadius: 5,
  },
  removeBtn: {
    padding: Spacing.one,
  },
  dimmed: {
    opacity: 0.5,
  },
})
