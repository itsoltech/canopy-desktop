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
  selected?: boolean
  onPress: () => void
}

const STATUS_COLORS: Record<Exclude<WorktreeAgentStatus, 'none'>, string> = StatusColors

export function WorktreeRow({
  worktree,
  repoRoot,
  selected = false,
  onPress,
}: WorktreeRowProps): React.ReactElement {
  const theme = useTheme()
  const { api } = useRemoteSession()
  const [removing, setRemoving] = useState(false)
  const canRemove = !worktree.isMain && repoRoot !== null && api !== null && !removing

  const runRemove = async (force: boolean): Promise<void> => {
    if (!api || !repoRoot) return
    setRemoving(true)
    try {
      const result = await api.worktree.remove({ repoRoot, path: worktree.path, force })
      if (result?.leftoverPath) {
        Alert.alert(
          'Worktree removed with leftovers',
          `Some files are still in use by another process and were left at:\n${result.leftoverPath}`,
        )
      }
    } catch (e) {
      Alert.alert('Could not remove worktree', e instanceof Error ? e.message : String(e))
    } finally {
      setRemoving(false)
    }
  }

  const confirmRemove = async (): Promise<void> => {
    if (!canRemove || !api || !repoRoot) return
    // Informed consent BEFORE the host tears anything down: the preflight reports
    // what a safe removal needs (uncommitted changes, unmerged commits,
    // submodules), and only an explicit confirmation authorizes --force. The host
    // rejects an unconsented force-required removal, so a stale/failed preflight
    // degrades safely.
    let forceRequired = true
    let warnings: string[] = []
    try {
      const preflight = await api.worktree.prepareRemove({ repoRoot, path: worktree.path })
      forceRequired = preflight.forceRequired
      warnings = preflight.warnings
    } catch {
      // Preflight unavailable (older host or broken/ghost worktree) — mirror the
      // desktop confirmWorktreeRemoval() contract and fail CLOSED: warn that the
      // state cannot be verified and send force only after destructive consent
      // (the host guard rejects an unconsented removal anyway).
      warnings = [
        'The worktree state could not be verified (broken checkout or older host) — files inside may include unsaved work.',
      ]
    }
    const message = forceRequired
      ? `${warnings.join('\n')}\n\nForce-remove "${worktree.branch}" anyway?`
      : `Remove "${worktree.branch}"?`
    Alert.alert('Remove worktree', message, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: forceRequired ? 'Force remove' : 'Remove',
        style: 'destructive',
        onPress: () => void runRemove(forceRequired),
      },
    ])
  }

  const agentStatus = worktree.agentStatus ?? 'none'

  return (
    <Pressable
      onPress={removing ? undefined : onPress}
      style={({ pressed }) => [
        styles.row,
        selected && { backgroundColor: theme.backgroundSelected },
        pressed && !removing && styles.pressed,
        removing && styles.dimmed,
      ]}
      accessibilityRole="button"
      accessibilityState={{ selected }}
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
    minHeight: 48,
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
