import { version } from 'expo/package.json'
import { Image } from 'expo-image'
import React from 'react'
import { useColorScheme, StyleSheet } from 'react-native'

import { ThemedText } from './themed-text'
import { ThemedView } from './themed-view'

import expoBadgeWhite from '@/assets/images/expo-badge-white.png'
import expoBadge from '@/assets/images/expo-badge.png'
import { Spacing } from '@/constants/theme'

export function WebBadge(): React.ReactElement {
  const scheme = useColorScheme()

  return (
    <ThemedView style={styles.container}>
      <ThemedText type="code" themeColor="textSecondary" style={styles.versionText}>
        v{version}
      </ThemedText>
      <Image source={scheme === 'dark' ? expoBadgeWhite : expoBadge} style={styles.badgeImage} />
    </ThemedView>
  )
}

const styles = StyleSheet.create({
  container: {
    padding: Spacing.five,
    alignItems: 'center',
    gap: Spacing.two,
  },
  versionText: {
    textAlign: 'center',
  },
  badgeImage: {
    width: 123,
    aspectRatio: 123 / 24,
  },
})
