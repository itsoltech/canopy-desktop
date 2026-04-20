import { NativeTabs } from 'expo-router/unstable-native-tabs'
import React from 'react'

import exploreIcon from '@/assets/images/tabIcons/explore.png'
import homeIcon from '@/assets/images/tabIcons/home.png'
import { Colors } from '@/constants/theme'
import { useColorScheme } from '@/hooks/use-color-scheme'

export default function AppTabs(): React.ReactElement {
  const scheme = useColorScheme()
  const colors = Colors[scheme]

  return (
    <NativeTabs
      backgroundColor={colors.background}
      indicatorColor={colors.backgroundElement}
      labelStyle={{ selected: { color: colors.text } }}
    >
      <NativeTabs.Trigger name="index">
        <NativeTabs.Trigger.Label>Instances</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon src={homeIcon} renderingMode="template" />
      </NativeTabs.Trigger>

      <NativeTabs.Trigger name="settings">
        <NativeTabs.Trigger.Label>Settings</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon src={exploreIcon} renderingMode="template" />
      </NativeTabs.Trigger>
    </NativeTabs>
  )
}
