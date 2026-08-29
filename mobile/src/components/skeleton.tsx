import { useEffect } from 'react'
import { type DimensionValue, type StyleProp, type ViewStyle } from 'react-native'
import Animated, {
  cancelAnimation,
  Easing,
  useAnimatedStyle,
  useReducedMotion,
  useSharedValue,
  withRepeat,
  withTiming,
} from 'react-native-reanimated'

import { useTheme } from '@/hooks/use-theme'

type SkeletonProps = {
  width?: DimensionValue
  height?: number
  radius?: number
  style?: StyleProp<ViewStyle>
}

/**
 * A muted placeholder block with a gentle opacity pulse, used to reserve
 * layout space while remote data loads (avoids content popping in / shifting).
 */
export function Skeleton({
  width = '100%',
  height = 16,
  radius = 6,
  style,
}: SkeletonProps): React.ReactElement {
  const theme = useTheme()
  const reduceMotion = useReducedMotion()
  const pulse = useSharedValue(0.4)

  useEffect(() => {
    // Respect the OS "Reduce Motion" setting: hold a static muted opacity
    // instead of the looping pulse (CLAUDE.md accessibility rule).
    if (reduceMotion) {
      cancelAnimation(pulse)
      pulse.value = 0.7
      return
    }
    pulse.value = withRepeat(
      withTiming(1, { duration: 900, easing: Easing.inOut(Easing.ease) }),
      -1,
      true,
    )
    return () => cancelAnimation(pulse)
  }, [pulse, reduceMotion])

  const animatedStyle = useAnimatedStyle(() => ({ opacity: pulse.value }))

  return (
    <Animated.View
      style={[
        { width, height, borderRadius: radius, backgroundColor: theme.backgroundElement },
        animatedStyle,
        style,
      ]}
    />
  )
}
