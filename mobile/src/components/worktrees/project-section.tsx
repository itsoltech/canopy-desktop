import { useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { Pressable, StyleSheet, View } from 'react-native'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { WorktreeRow } from '@/components/worktrees/worktree-row'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'
import type { ProjectSnapshot, WorktreeSnapshot } from '@/lib/mock/snapshot-types'

type ProjectSectionProps = {
  project: ProjectSnapshot
  onWorktreePress: (project: ProjectSnapshot, worktree: WorktreeSnapshot) => void
}

export function ProjectSection({
  project,
  onWorktreePress,
}: ProjectSectionProps): React.ReactElement {
  const theme = useTheme()
  const router = useRouter()
  const canCreate = project.isGitRepo && project.repoRoot !== null

  const openCreate = (): void => {
    if (!canCreate || !project.repoRoot) return
    router.push({
      // Typed routes aren't regenerated until the next `expo start`, so
      // the cast is needed when the target route file was just created.
      pathname: '/worktree/new' as never,
      params: { projectId: project.id },
    })
  }

  return (
    <View style={styles.section}>
      <View style={styles.header}>
        <View style={styles.headerRow}>
          <View style={styles.headerText}>
            <ThemedText type="smallBold" style={styles.name} numberOfLines={1}>
              {project.name}
            </ThemedText>
            <ThemedText type="code" themeColor="textSecondary" numberOfLines={1}>
              {project.path}
            </ThemedText>
          </View>
          {canCreate && (
            <Pressable
              onPress={openCreate}
              style={({ pressed }) => [styles.addBtn, pressed && styles.pressed]}
              accessibilityLabel={`Create worktree in ${project.name}`}
            >
              <SymbolView
                name={{ ios: 'plus', android: 'add', web: 'add' }}
                size={14}
                weight="semibold"
                tintColor={theme.text}
              />
              <ThemedText type="smallBold">new</ThemedText>
            </Pressable>
          )}
        </View>
      </View>
      {project.worktrees.length === 0 ? (
        <ThemedView type="backgroundElement" style={styles.empty}>
          <ThemedText type="small" themeColor="textSecondary">
            {project.isGitRepo ? 'No worktrees' : 'Not a git repository'}
          </ThemedText>
        </ThemedView>
      ) : (
        <ThemedView type="backgroundElement" style={styles.rows}>
          {project.worktrees.map((w, idx) => (
            <View key={w.path}>
              {idx > 0 && <View style={styles.divider} />}
              <WorktreeRow
                worktree={w}
                repoRoot={project.repoRoot}
                onPress={() => onWorktreePress(project, w)}
              />
            </View>
          ))}
        </ThemedView>
      )}
    </View>
  )
}

const styles = StyleSheet.create({
  section: {
    gap: Spacing.two,
  },
  header: {
    paddingHorizontal: Spacing.three,
    gap: 2,
  },
  headerRow: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: Spacing.two,
  },
  headerText: {
    flex: 1,
    gap: 2,
  },
  name: {
    fontSize: 15,
  },
  addBtn: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.one,
    paddingHorizontal: Spacing.two,
    paddingVertical: Spacing.one,
    borderRadius: Spacing.two,
    backgroundColor: 'rgba(127, 127, 127, 0.15)',
  },
  pressed: {
    opacity: 0.6,
  },
  rows: {
    borderRadius: Spacing.three,
    overflow: 'hidden',
  },
  divider: {
    height: StyleSheet.hairlineWidth,
    backgroundColor: 'rgba(127, 127, 127, 0.2)',
    marginLeft: Spacing.three + Spacing.three,
  },
  empty: {
    borderRadius: Spacing.three,
    padding: Spacing.three,
  },
})
