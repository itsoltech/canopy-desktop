import { useState } from 'react'
import { Dimensions, StyleSheet } from 'react-native'
import Animated, { Easing, Keyframe } from 'react-native-reanimated'
import { scheduleOnRN } from 'react-native-worklets'

const INITIAL_SCALE_FACTOR = Dimensions.get('screen').height / 90
const DURATION = 600

export function AnimatedSplashOverlay(): React.ReactElement | null {
  const [visible, setVisible] = useState(true)

  if (!visible) return null

  const splashKeyframe = new Keyframe({
    0: {
      transform: [{ scale: INITIAL_SCALE_FACTOR }],
      opacity: 1,
    },
    20: {
      opacity: 1,
    },
    70: {
      opacity: 0,
      easing: Easing.elastic(0.7),
    },
    100: {
      opacity: 0,
      transform: [{ scale: 1 }],
      easing: Easing.elastic(0.7),
    },
  })

  return (
    <Animated.View
      entering={splashKeyframe.duration(DURATION).withCallback((finished) => {
        'worklet'
        if (finished) {
          scheduleOnRN(setVisible, false)
        }
      })}
      style={styles.backgroundSolidColor}
    />
  )
}

const styles = StyleSheet.create({
  backgroundSolidColor: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: '#FFFFFF',
    zIndex: 1000,
  },
})
