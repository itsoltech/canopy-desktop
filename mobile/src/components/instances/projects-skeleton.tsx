import { StyleSheet, View } from 'react-native'
import Animated, { FadeOut } from 'react-native-reanimated'

import { Skeleton } from '@/components/skeleton'
import { Spacing } from '@/constants/theme'

/**
 * Placeholder shaped like the instance detail's PROJECTS & WORKTREES list, so
 * the layout footprint is reserved while the host snapshot is still hydrating.
 * Mirrors `ProjectSection`'s spacing (header + a rounded rows block).
 */
export function ProjectsSkeleton(): React.ReactElement {
  return (
    <Animated.View exiting={FadeOut} style={styles.container}>
      {[0, 1, 2].map((i) => (
        <View key={i} style={styles.section}>
          <View style={styles.header}>
            <Skeleton width={140} height={15} radius={4} />
            <Skeleton width={220} height={12} radius={4} />
          </View>
          <Skeleton width="100%" height={52} radius={Spacing.three} />
        </View>
      ))}
    </Animated.View>
  )
}

const styles = StyleSheet.create({
  container: {
    paddingHorizontal: Spacing.four,
    gap: Spacing.four,
  },
  section: {
    gap: Spacing.two,
  },
  header: {
    paddingHorizontal: Spacing.three,
    gap: Spacing.one,
  },
})
