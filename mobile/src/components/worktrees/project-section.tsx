import { StyleSheet, View } from 'react-native'

import { WorktreeRow } from '@/components/worktrees/worktree-row'
import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { Spacing } from '@/constants/theme'
import type { ProjectSnapshot, WorktreeSnapshot } from '@/lib/mock/snapshot-types'

type ProjectSectionProps = {
  project: ProjectSnapshot
  onWorktreePress: (project: ProjectSnapshot, worktree: WorktreeSnapshot) => void
}

export function ProjectSection({ project, onWorktreePress }: ProjectSectionProps) {
  return (
    <View style={styles.section}>
      <View style={styles.header}>
        <ThemedText type="smallBold" style={styles.name} numberOfLines={1}>
          {project.name}
        </ThemedText>
        <ThemedText type="code" themeColor="textSecondary" numberOfLines={1}>
          {project.path}
        </ThemedText>
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
              <WorktreeRow worktree={w} onPress={() => onWorktreePress(project, w)} />
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
  name: {
    fontSize: 15,
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
