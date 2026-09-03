import { ScrollView, StyleSheet, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { ProjectSection } from '@/components/worktrees/project-section'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'
import type { ProjectSnapshot, WorktreeSnapshot } from '@/lib/remote/protocol/state-snapshot'

type WorktreeSidebarProps = {
  width: number
  instanceName: string
  projects: ProjectSnapshot[]
  activeWorktreePath: string
  connected: boolean
  onWorktreePress: (project: ProjectSnapshot, worktree: WorktreeSnapshot) => void
}

export function WorktreeSidebar({
  width,
  instanceName,
  projects,
  activeWorktreePath,
  connected,
  onWorktreePress,
}: WorktreeSidebarProps): React.ReactElement {
  const theme = useTheme()

  return (
    <ThemedView
      type="background"
      style={[styles.container, { width, borderRightColor: theme.backgroundSelected }]}
    >
      <SafeAreaView style={styles.safeArea} edges={['top', 'left', 'bottom']}>
        <View style={styles.header}>
          <ThemedText type="smallBold" style={styles.title}>
            Worktrees
          </ThemedText>
          <ThemedText type="small" themeColor="textSecondary" numberOfLines={1}>
            {instanceName}
          </ThemedText>
        </View>

        <ScrollView
          style={styles.scroll}
          contentContainerStyle={styles.content}
          showsVerticalScrollIndicator={false}
        >
          {projects.length === 0 ? (
            <ThemedView type="backgroundSelected" style={styles.empty}>
              <ThemedText type="small" themeColor="textSecondary">
                {connected ? 'No projects open on the host' : 'Waiting for host data…'}
              </ThemedText>
            </ThemedView>
          ) : (
            projects.map((project) => (
              <ProjectSection
                key={project.id}
                project={project}
                selectedWorktreePath={activeWorktreePath}
                onWorktreePress={onWorktreePress}
              />
            ))
          )}
        </ScrollView>
      </SafeAreaView>
    </ThemedView>
  )
}

const styles = StyleSheet.create({
  container: {
    flexShrink: 0,
    borderRightWidth: StyleSheet.hairlineWidth,
  },
  safeArea: {
    flex: 1,
  },
  header: {
    paddingHorizontal: Spacing.three,
    paddingTop: Spacing.three,
    paddingBottom: Spacing.three,
    gap: Spacing.half,
  },
  title: {
    fontSize: 18,
    lineHeight: 24,
  },
  scroll: {
    flex: 1,
  },
  content: {
    paddingHorizontal: Spacing.two,
    paddingBottom: Spacing.four,
    gap: Spacing.four,
  },
  empty: {
    borderRadius: Spacing.three,
    padding: Spacing.three,
  },
})
