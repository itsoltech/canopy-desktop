import { StyleSheet, View } from 'react-native'
import Animated, { FadeOut, useReducedMotion } from 'react-native-reanimated'

import { Skeleton } from '@/components/skeleton'
import { Spacing } from '@/constants/theme'

/**
 * Placeholder shaped like the instance detail's PROJECTS & WORKTREES list, so
 * the layout footprint is reserved while the host snapshot is still hydrating.
 * Mirrors `ProjectSection`'s spacing (header + a rounded rows block).
 *
 * Announced to screen readers as a single "Loading projects" busy element
 * (descendants hidden) and the fade-out is dropped under Reduce Motion.
 */
export function ProjectsSkeleton(): React.ReactElement {
  const reduceMotion = useReducedMotion()
  return (
    <Animated.View
      exiting={reduceMotion ? undefined : FadeOut}
      style={styles.container}
      accessible
      accessibilityLabel="Loading projects"
      accessibilityState={{ busy: true }}
      importantForAccessibility="no-hide-descendants"
    >
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
