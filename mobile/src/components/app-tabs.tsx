import { ThemeProvider } from 'expo-router/react-navigation'
import { NativeTabs } from 'expo-router/unstable-native-tabs'
import React from 'react'

import { CanopyDarkTheme, CanopyLightTheme } from '@/constants/navigation-theme'
import { Colors } from '@/constants/theme'
import { useColorScheme } from '@/hooks/use-color-scheme'

import { TABS } from './tabs/tab-config'

export default function AppTabs(): React.ReactElement {
  const scheme = useColorScheme()
  const colors = Colors[scheme]

  // NativeTabs needs its own ThemeProvider, otherwise the native tab bar
  // flashes the default (light) theme during tab/screen transitions on iOS 26
  // (expo/expo#44033, #41360). The explicit color props set the steady-state
  // appearance; the ThemeProvider keeps it themed through the transition.
  return (
    <ThemeProvider value={scheme === 'dark' ? CanopyDarkTheme : CanopyLightTheme}>
      <NativeTabs
        backgroundColor={colors.background}
        indicatorColor={colors.backgroundElement}
        labelStyle={{ selected: { color: colors.text } }}
      >
        {TABS.map((tab) => (
          <NativeTabs.Trigger key={tab.name} name={tab.name}>
            <NativeTabs.Trigger.Label>{tab.label}</NativeTabs.Trigger.Label>
            <NativeTabs.Trigger.Icon src={tab.icon} renderingMode="template" />
          </NativeTabs.Trigger>
        ))}
      </NativeTabs>
    </ThemeProvider>
  )
}
